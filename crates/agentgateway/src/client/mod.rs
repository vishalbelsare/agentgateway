mod dns;
mod hyperrustls;

use std::str::FromStr;
use std::task;

use ::http::Uri;
use ::http::uri::{Authority, Scheme};
use hyper_util_fork::rt::TokioIo;
use rustls_pki_types::{DnsName, ServerName};
use tracing::event;

use crate::http::backendtls::BackendTLS;
use crate::proxy::ProxyError;
use crate::transport::hbone::WorkloadKey;
use crate::transport::stream::{LoggingMode, Socket};
use crate::transport::{hbone, stream};
use crate::types::agent::Target;
use crate::*;

#[derive(Clone)]
pub struct Client {
	resolver: Arc<dns::CachedResolver>,
	client: hyper_util_fork::client::legacy::Client<Connector, http::Body, PoolKey>,
}

impl Debug for Client {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Client").finish()
	}
}

pub struct Call {
	pub req: http::Request,
	pub target: Target,
	pub transport: Transport,
}

#[derive(Default, Debug, Clone, Hash, PartialEq, Eq)]
pub enum Transport {
	#[default]
	Plaintext,
	Tls(BackendTLS),
	Hbone(Option<BackendTLS>, Identity),
}
impl Transport {
	pub fn name(&self) -> &'static str {
		match self {
			Transport::Plaintext => "plaintext",
			Transport::Tls(_) => "tls",
			Transport::Hbone(_, _) => "hbone",
		}
	}
}

impl From<Option<BackendTLS>> for Transport {
	fn from(tls: Option<BackendTLS>) -> Self {
		if let Some(tls) = tls {
			client::Transport::Tls(tls)
		} else {
			client::Transport::Plaintext
		}
	}
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct PoolKey(Target, SocketAddr, Transport, ::http::Version);

impl Transport {
	pub fn scheme(&self) -> Scheme {
		match self {
			Transport::Plaintext => Scheme::HTTP,
			// TODO: make sure this is right, envoy had all sorts of issues around this.
			Transport::Tls(_) => Scheme::HTTPS,
			Transport::Hbone(inner, _) => {
				if inner.is_some() {
					Scheme::HTTPS
				} else {
					// It is a tunnel, so the fact its HTTPS is transparent!
					Scheme::HTTP
				}
			},
		}
	}
}

#[derive(Debug, Clone)]
struct Connector {
	hbone_pool: Option<agent_hbone::pool::WorkloadHBONEPool<hbone::WorkloadKey>>,
}

impl tower::Service<::http::Extensions> for Connector {
	type Response = TokioIo<crate::transport::stream::Socket>;
	type Error = crate::http::Error;
	type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

	fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
		Poll::Ready(Ok(()))
	}

	fn call(&mut self, mut dst: ::http::Extensions) -> Self::Future {
		let it = self.clone();

		Box::pin(async move {
			let PoolKey(target, ep, transport, _) =
				dst.remove::<PoolKey>().expect("pool key must be set");

			match transport {
				Transport::Plaintext => {
					let mut res = Socket::dial(ep)
						.await
						.context("http call failed")
						.map_err(crate::http::Error::new)?;
					res.with_logging(LoggingMode::Upstream);
					Ok(TokioIo::new(res))
				},
				Transport::Tls(tls) => {
					let server_name = if let Some(h) = tls.hostname_override {
						h
					} else {
						match target {
							Target::Address(_) => ServerName::IpAddress(ep.ip().into()),
							Target::Hostname(host, _) => ServerName::DnsName(
								DnsName::try_from(host.to_string()).expect("TODO: hostname conversion failed"),
							),
						}
					};

					let mut https = self::hyperrustls::HttpsConnector {
						tls_config: tls.config.clone(),
						server_name,
					};

					let mut res = https.call(ep).await.map_err(crate::http::Error::new)?;
					res.with_logging(LoggingMode::Upstream);
					Ok(TokioIo::new(res))
				},
				Transport::Hbone(inner, identity) => {
					if inner.is_some() {
						return Err(crate::http::Error::new(anyhow::anyhow!(
							"todo: inner TLS is not currently supported"
						)));
					}
					let uri = Uri::builder()
						.scheme(Scheme::HTTPS)
						.authority(ep.to_string())
						.path_and_query("/")
						.build()
						.expect("todo");
					tracing::debug!("will use HBONE");
					let req = ::http::Request::builder()
						.uri(uri)
						.method(hyper::Method::CONNECT)
						.version(hyper::Version::HTTP_2)
						.body(())
						.expect("builder with known status code should not fail");

					let pool_key = Box::new(WorkloadKey {
						dst_id: vec![identity],
						dst: SocketAddr::from((ep.ip(), 15008)),
					});
					let mut pool = it
						.hbone_pool
						.clone()
						.ok_or_else(|| crate::http::Error::new(anyhow::anyhow!("hbone pool disabled")))?;

					let upgraded = Box::pin(pool.send_request_pooled(&pool_key, req))
						.await
						.map_err(crate::http::Error::new)?;
					let rw = agent_hbone::RWStream {
						stream: upgraded,
						buf: Default::default(),
					};
					let mut socket = Socket::from_hbone(Arc::new(stream::Extension::new()), pool_key.dst, rw);
					socket.with_logging(LoggingMode::Upstream);
					Ok(TokioIo::new(socket))
				},
			}
		})
	}
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Config {
	pub resolver_cfg: ResolverConfig,
	pub resolver_opts: ResolverOpts,
}

impl Client {
	pub fn new(
		cfg: &Config,
		hbone_pool: Option<agent_hbone::pool::WorkloadHBONEPool<hbone::WorkloadKey>>,
	) -> Client {
		let resolver = dns::CachedResolver::new(cfg.resolver_cfg.clone(), cfg.resolver_opts.clone());
		let client =
			::hyper_util_fork::client::legacy::Client::builder(::hyper_util::rt::TokioExecutor::new())
				.timer(hyper_util::rt::tokio::TokioTimer::new())
				.build_with_pool_key(Connector { hbone_pool });
		Client {
			resolver: Arc::new(resolver),
			client,
		}
	}

	pub async fn simple_call(&self, req: http::Request) -> Result<http::Response, ProxyError> {
		let host = req
			.uri()
			.host()
			.ok_or_else(|| ProxyError::ProcessingString("no hostname set".to_string()))?;
		let scheme = req
			.uri()
			.scheme()
			.ok_or_else(|| ProxyError::ProcessingString("no scheme set".to_string()))?;
		let port = req
			.uri()
			.port()
			.map(|p| p.as_u16())
			.unwrap_or_else(|| if scheme == &Scheme::HTTPS { 443 } else { 80 });
		let transport = if scheme == &Scheme::HTTPS {
			Transport::Tls(http::backendtls::SYSTEM_TRUST.clone())
		} else {
			Transport::Plaintext
		};
		let target = Target::try_from((host, port))
			.map_err(|e| ProxyError::ProcessingString(format!("failed to parse host: {e}")))?;
		self
			.call(Call {
				req,
				target,
				transport,
			})
			.await
	}

	pub async fn call(&self, call: Call) -> Result<http::Response, ProxyError> {
		let start = std::time::Instant::now();
		let Call {
			mut req,
			target,
			transport,
		} = call;
		let dest = match &target {
			Target::Address(addr) => *addr,
			Target::Hostname(hostname, port) => {
				let ip = self
					.resolver
					.resolve(hostname.clone())
					.await
					.map_err(|_| ProxyError::DnsResolution)?;
				SocketAddr::from((ip, *port))
			},
		};
		http::modify_req_uri(&mut req, |uri| {
			let scheme = transport.scheme();
			// Strip the port from the hostname if its the default already
			// The hyper client does this for HTTP/1.1 but not for HTTP2
			if let Some(a) = uri.authority.as_mut()
				&& ((scheme == Scheme::HTTPS && a.port_u16() == Some(443))
					|| (scheme == Scheme::HTTP && a.port_u16() == Some(80)))
			{
				*a = Authority::from_str(a.host()).expect("host must be valid since it was already a host");
			}
			uri.scheme = Some(scheme);
			Ok(())
		})
		.map_err(ProxyError::Processing)?;
		let version = req.version();
		let transport_name = transport.name();
		let target_name = target.to_string();
		let key = PoolKey(target, dest, transport, version);
		trace!(?req, ?key, "sending request");
		req.extensions_mut().insert(key);
		let method = req.method().clone();
		let uri = req.uri().clone();
		let path = uri.path();
		let host = uri.authority().to_owned();
		// const TIMEOUT: Option<Duration> = Some(Duration::from_secs(5));
		const TIMEOUT: Option<Duration> = None;
		let resp = match TIMEOUT {
			Some(to) => match tokio::time::timeout(to, self.client.request(req)).await {
				Ok(res) => res.map_err(ProxyError::UpstreamCallFailed),
				Err(_) => Err(ProxyError::RequestTimeout),
			},
			None => self
				.client
				.request(req)
				.await
				.map_err(ProxyError::UpstreamCallFailed),
		};
		let dur = format!("{}ms", start.elapsed().as_millis());
		event!(
			target: "upstream request",
			parent: None,
			tracing::Level::DEBUG,

			target = %target_name,
			endpoint = %dest,
			transport = %transport_name,

			http.method = %method,
			http.host = host.as_ref().map(display),
			http.path = %path,
			http.version = ?version,
			http.status = resp.as_ref().ok().map(|s| s.status().as_u16()).unwrap_or_default(),

			duration = dur,
		);
		Ok(resp?.map(http::Body::new))
	}
}
