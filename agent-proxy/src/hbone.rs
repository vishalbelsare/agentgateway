use crate::ConfigStore;
use crate::stream::{Extension, Socket};
use crate::types::discovery;
use crate::types::discovery::{Identity, NetworkAddress};
use agent_core::prelude::Strng;
use agent_hbone::Key;
use async_trait::async_trait;
use http::Uri;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioIo;
use std::fmt::{Display, Formatter};
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;
use std::{fmt, task};

#[derive(Debug, Clone)]
pub struct HBONEConnector {
	http: HttpConnector,
	hbone: Option<Inner>,
}

#[derive(Debug, Clone)]
pub struct Inner {
	pool: agent_hbone::pool::WorkloadHBONEPool<WorkloadKey>,
	state: ConfigStore,
	network: Strng,
}

impl HBONEConnector {
	pub fn new(
		state: ConfigStore,
		cfg: &crate::Config,
		local_workload_information: Arc<LocalWorkloadInformation>,
	) -> Self {
		let pool = agent_hbone::pool::WorkloadHBONEPool::new(cfg.hbone.clone(), local_workload_information);
		let inner = Inner { pool, state, network: cfg.network.clone() };
		Self { http: Self::http(), hbone: Some(inner) }
	}
	pub fn new_disabled() -> Self {
		Self { http: Self::http(), hbone: None }
	}
	fn http() -> HttpConnector {
		let mut http = HttpConnector::new();
		http.set_keepalive(Some(Duration::from_secs(90)));
		http
	}
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct WorkloadKey {
	pub dst_id: Vec<Identity>,
	pub dst: SocketAddr,
}

impl Key for WorkloadKey {
	fn dest(&self) -> SocketAddr {
		self.dst
	}
}

impl Display for WorkloadKey {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "{}[", self.dst,)?;
		for i in &self.dst_id {
			write!(f, "{i}")?;
		}
		write!(f, "]")
	}
}

impl tower::Service<Uri> for HBONEConnector {
	type Response = TokioIo<crate::stream::Socket>;
	type Error = crate::http::Error;
	type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

	fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
		Poll::Ready(Ok(()))
	}

	fn call(&mut self, dst: Uri) -> Self::Future {
		let mut it = self.clone();

		Box::pin(async move {
			let socket: IpAddr = dst
				.host()
				.expect("must have a host")
				.parse()
				.expect("must be a socket");

			let port = dst.port_u16().expect("must have a port");
			if let Some(mut hbone) = it.hbone {
				let wl = {
					let st = hbone.state.read_discovery();

					st.workloads
						.find_address(&NetworkAddress { network: hbone.network.clone(), address: socket })
				};
				if let Some(wl) = wl {
					if wl.protocol == discovery::InboundProtocol::HBONE {
						let dst = SocketAddr::from((socket, port));
						tracing::error!("howardjohn: SHOULD HBONE");
						let req = ::http::Request::builder()
							.uri(format!("{dst}"))
							.method(hyper::Method::CONNECT)
							.version(hyper::Version::HTTP_2)
							.body(())
							.expect("builder with known status code should not fail");

						let pool_key = Box::new(WorkloadKey {
							dst_id: vec![wl.identity()],
							dst: SocketAddr::from((socket, 15008)),
						});

						let upgraded = Box::pin(hbone.pool.send_request_pooled(&pool_key, req))
							.await
							.map_err(crate::http::Error::new)?;
						let rw = agent_hbone::RWStream { stream: upgraded, buf: Default::default() };
						let socket = Socket::from_hbone(Arc::new(Extension::new()), dst, rw);
						return Ok(TokioIo::new(socket));
					}
				}
			}
			let res = it.http.call(dst).await.map_err(crate::http::Error::new)?;
			Ok(TokioIo::new(crate::stream::Socket::from_tcp(res.into_inner()).map_err(crate::http::Error::new)?))
		})
	}
}

impl LocalWorkloadInformation {}

#[async_trait]
impl agent_hbone::pool::CertificateFetcher<WorkloadKey> for LocalWorkloadInformation {
	async fn fetch_certificate(&self, key: WorkloadKey) -> anyhow::Result<Arc<rustls::client::ClientConfig>> {
		Err(anyhow::anyhow!("TODO"))
	}
}

pub struct LocalWorkloadInformation {}

impl LocalWorkloadInformation {
	pub fn new() -> LocalWorkloadInformation {
		LocalWorkloadInformation {}
	}

	pub async fn fetch_server_config(&self) -> anyhow::Result<Arc<rustls::server::ServerConfig>> {
		Err(anyhow::anyhow!("TODO"))
	}
}
