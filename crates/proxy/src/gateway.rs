use crate::ProxyInputs;
use crate::ext_proc::ExtProc;
use crate::http::{Authority, HeaderName, Request, Response, Scheme, StatusCode, Uri, filters};
use crate::store::Event;
use crate::stream::{Extension, Socket, TLSConnectionInfo, TcpConnectionInfo};
use crate::types::agent;
use crate::*;
use agent_core::drain;
use agent_core::drain::DrainWatcher;
use anyhow::anyhow;
use futures_util::FutureExt;
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use hyper_util::server::conn::auto;
use itertools::Itertools;
use rand::Rng;
use rand::seq::IndexedRandom;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::{AbortHandle, JoinSet};
use tokio_stream::StreamExt;
use tracing::{Instrument, debug, event, info, info_span, trace};
use types::agent::*;
use types::discovery::*;

#[derive(Debug, Clone)]
pub(super) struct Upstream {
	client: revproxy::ReverseProxy<hbone::HBONEConnector>,
}

pub(super) struct Gateway {
	drain: DrainWatcher,
	pi: Arc<ProxyInputs>,
	upstream: Upstream,
}

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

impl<T> Stream for T where T: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

impl Gateway {
	pub(super) fn new(pi: Arc<ProxyInputs>, drain: DrainWatcher) -> Gateway {
		let connector = if pi.cfg.backend_mesh {
			hbone::HBONEConnector::new(
				pi.store.clone(),
				&pi.cfg,
				pi.local_workload_information.clone(),
			)
		} else {
			hbone::HBONEConnector::new_disabled()
		};
		Gateway {
			upstream: Upstream {
				client: revproxy::ReverseProxy::new(
					pooling_client::<http::Body>(connector.clone()),
					pooling_h2_client::<http::Body>(connector.clone()),
				),
			},
			drain,
			pi,
		}
	}

	pub(super) async fn run(self) {
		let mut js = JoinSet::new();
		let (initial_binds, mut binds) = {
			let binds = self.pi.store.read_binds();
			(binds.all(), binds.subscribe())
		};
		let mut active: HashMap<SocketAddr, AbortHandle> = HashMap::new();
		let mut handle_bind = |js: &mut JoinSet<anyhow::Result<()>>, b: Event<Arc<Bind>>| {
			let b = match b {
				Event::Add(b) => b,
				Event::Remove(to_remove) => {
					if let Some(h) = active.remove(&to_remove.address) {
						h.abort();
					}
					return;
				},
			};
			if active.contains_key(&b.address) {
				debug!("bind already exists");
				return;
			}

			debug!("add bind {}", b.address);
			let task = js.spawn(
				Self::run_bind(
					self.upstream.clone(),
					self.pi.clone(),
					self.drain.clone(),
					b.clone(),
				)
				.in_current_span(),
			);
			active.insert(b.address, task);
		};
		for bind in initial_binds {
			handle_bind(&mut js, Event::Add(bind))
		}
		loop {
			tokio::select! {
				Some(res) = binds.next() => {
					handle_bind(&mut js, res.expect("TODO"));
				}
				Some(res) = js.join_next() => {
					warn!("bind complete {res:?}");
				}
			}
		}
	}

	pub(super) async fn run_bind(
		client: Upstream,
		pi: Arc<ProxyInputs>,
		drain: DrainWatcher,
		b: Arc<Bind>,
	) -> anyhow::Result<()> {
		let self_termination_deadline = pi.cfg.self_termination_deadline;
		let Bind {
			name,
			address: _,
			listeners: _,
		} = &*b;
		let listener = TcpListener::bind(b.address).await?; // TODO: nodelay
		info!(bind = name.as_str(), "started bind");
		let accept = |drain: DrainWatcher, force_shutdown: watch::Receiver<()>| async move {
			while let Ok((stream, _peer)) = listener.accept().await {
				let stream = Socket::from_tcp(stream).expect("todo");
				let pi = pi.clone();
				let drain = drain.clone();
				let start = Instant::now();
				let mut force_shutdown = force_shutdown.clone();
				let name = name.clone();
				let client = client.clone();
				tokio::spawn(async move {
					debug!(bind=?name, "connection started");
					tokio::select! {
						_ = force_shutdown.changed() => {
							debug!(bind=?name, "connection forcefully terminated");
						}
						_ = Self::proxy_bind(client.clone(), name.clone(), stream, pi) => {}
					}
					debug!(bind=?name, dur=?start.elapsed(), "connection completed");
					// Mark we are done with the connection, so drain can complete
					drop(drain);
				});
			}
		};

		drain::run_with_drain("bind".to_string(), drain, self_termination_deadline, accept).await;
		Ok(())
	}

	async fn proxy_bind(
		upstream: Upstream,
		bind_name: BindName,
		raw_stream: Socket,
		inputs: Arc<ProxyInputs>,
	) -> anyhow::Result<()> {
		if bind_protocol(inputs.clone(), bind_name.clone()) == BindProtocol::Hbone {
			return Self::terminate_hbone(upstream, bind_name, inputs, raw_stream).await;
		}
		let (selected_listener, stream) =
			Self::maybe_terminate_tls(inputs.clone(), raw_stream, bind_name.clone()).await?;
		Self::proxy(upstream, bind_name, inputs, selected_listener, stream).await
	}

	async fn proxy(
		upstream: Upstream,
		bind_name: BindName,
		inputs: Arc<ProxyInputs>,
		selected_listener: Option<Arc<Listener>>,
		stream: Socket,
	) -> anyhow::Result<()> {
		let target_address = stream.target_address();
		let proxy = HTTPProxy {
			upstream,
			bind_name,
			inputs,
			selected_listener,
			target_address,
		};
		let server = auto_server();
		let connection = Arc::new(stream.get_ext());
		let serve = server // TODO: tune all optinos
			.serve_connection_with_upgrades(
				TokioIo::new(stream),
				hyper::service::service_fn(move |req| {
					let proxy = proxy.clone();
					let connection = connection.clone();
					async move { proxy.proxy(connection, req).map(Ok::<_, Infallible>).await }
				}),
			);
		let res = serve.await;
		match res {
			Ok(_) => Ok(()),
			Err(e) => {
				anyhow::bail!("{e}");
			},
		}
	}

	async fn maybe_terminate_tls(
		inp: Arc<ProxyInputs>,
		raw_stream: Socket,
		bind: BindName,
	) -> anyhow::Result<(Option<Arc<Listener>>, Socket)> {
		let listeners = inp.store.read_binds().listeners(bind.clone()).unwrap();
		if bind_protocol(inp.clone(), bind.clone()) == BindProtocol::Http {
			return Ok((None, raw_stream));
		}
		let (ext, inner) = raw_stream.into_parts();
		let acceptor =
			tokio_rustls::LazyConfigAcceptor::new(rustls::server::Acceptor::default(), Box::new(inner));
		let start = acceptor.await?;
		let ch = start.client_hello();
		let best = listeners
			.best_match(ch.server_name().unwrap_or_default())
			.ok_or(anyhow!("no TLS listener match"))?;
		let cfg = best.protocol.tls().unwrap();
		let tls = start.into_stream(cfg).await?;
		Ok((Some(best), Socket::from_tls(ext, tls)?))
	}

	async fn terminate_hbone(
		upstream: Upstream,
		bind_name: BindName,
		inp: Arc<ProxyInputs>,
		raw_stream: Socket,
	) -> anyhow::Result<()> {
		let sc = inp.local_workload_information.fetch_server_config().await?;
		let tls = crate::transport::tls::accept(raw_stream, sc).await?;

		debug!("accepted connection");
		let cfg = inp.cfg.clone();
		let request_handler = move |req, ext| {
			Self::serve_connect(upstream.clone(), bind_name.clone(), inp.clone(), req, ext)
				.instrument(info_span!("inbound"))
		};
		// TODO proper drain and watch
		let (_trigger, watcher) = drain::new();

		let (_, force_shutdown) = watch::channel(());
		let ext = Arc::new(tls.get_ext());
		let serve_conn = agent_hbone::server::serve_connection(
			cfg.hbone.clone(),
			tls,
			ext,
			watcher,
			force_shutdown,
			request_handler,
		);
		serve_conn.await
	}
	/// serve_connect handles a single connection from a client.
	#[allow(clippy::too_many_arguments)]
	async fn serve_connect(
		upstream: Upstream,
		bind_name: BindName,
		pi: Arc<ProxyInputs>,
		req: agent_hbone::server::H2Request,
		ext: Arc<Extension>,
	) {
		debug!(?req, "received request");

		let hbone_addr = req
			.uri()
			.to_string()
			.as_str()
			.parse::<SocketAddr>()
			.map_err(|_| InboundError(anyhow::anyhow!("bad request"), StatusCode::BAD_REQUEST))
			.unwrap();
		let resp = req
			.send_response(build_response(StatusCode::OK))
			.await
			.unwrap();
		let con = agent_hbone::RWStream {
			stream: resp,
			buf: bytes::Bytes::new(),
		};

		let _ = Self::proxy(
			upstream,
			bind_name,
			pi,
			None,
			Socket::from_hbone(ext, hbone_addr, con),
		)
		.await;
	}
}

fn select_backend(route: &Route, _req: &Request) -> Option<RouteBackend> {
	route
		.backends
		.choose_weighted(&mut rand::rng(), |b| b.weight)
		.ok()
		.cloned()
}

fn apply_request_filters(
	filters: &[RouteFilter],
	path_match: &PathMatch,
	req: &mut Request,
) -> Option<Response> {
	debug!("before request filters: {:?}", req);
	for filter in filters {
		match filter {
			RouteFilter::RequestHeaderModifier(hm) => hm.apply(req.headers_mut()),
			RouteFilter::ResponseHeaderModifier { .. } => {},
			RouteFilter::UrlRewrite(rw) => rw.apply(req, path_match),
			RouteFilter::RequestRedirect(red) => {
				if let Some(direct_response) = red.apply(req, path_match) {
					return Some(direct_response);
				}
			},
			RouteFilter::RequestMirror(_) => {},
		}
	}
	None
}

fn get_mirrors(filters: &[RouteFilter]) -> Vec<filters::RequestMirror> {
	let mut res = vec![];
	for filter in filters {
		if let RouteFilter::RequestMirror(m) = filter {
			res.push(m.clone())
		}
	}
	res
}

fn apply_response_filters(filters: &[RouteFilter], resp: &mut Response) {
	for filter in filters {
		match filter {
			RouteFilter::ResponseHeaderModifier(rh) => rh.apply(resp.headers_mut()),
			RouteFilter::RequestHeaderModifier { .. } => {},
			RouteFilter::UrlRewrite { .. } => {},
			RouteFilter::RequestRedirect { .. } => {},
			RouteFilter::RequestMirror(_) => {},
		}
	}
}

fn select_best_route(
	inp: Arc<ProxyInputs>,
	dst: SocketAddr,
	listener: Arc<Listener>,
	request: &Request,
) -> Option<(Arc<Route>, PathMatch)> {
	// Order:
	// * "Exact" path match.
	// * "Prefix" path match with largest number of characters.
	// * Method match.
	// * Largest number of header matches.
	// * Largest number of query param matches.
	//
	// If ties still exist across multiple Routes, matching precedence MUST be
	// determined in order of the following criteria, continuing on ties:
	//
	//  * The oldest Route based on creation timestamp.
	//  * The Route appearing first in alphabetical order by "{namespace}/{name}".
	//
	// If ties still exist within an HTTPRoute, matching precedence MUST be granted
	// to the FIRST matching rule (in list order) with a match meeting the above
	// criteria.

	// Assume matches are ordered already (not true today)
	let host = get_host(request);
	// TODO: ensure we actually serve this service
	if matches!(listener.protocol, ListenerProtocol::HBONE) && listener.routes.is_empty() {
		let network = inp.cfg.network.clone();
		let svc = inp
			.store
			.read_discovery()
			.services
			.get_by_vip(&NetworkAddress {
				network,
				address: dst.ip(),
			})
			.unwrap();
		let default_route = Route {
			name: strng::new("waypoint-default"),
			group_name: strng::new("waypoint-default"),
			section_name: None,
			hostnames: vec![],
			matches: vec![],
			filters: vec![],
			policies: None,
			backends: vec![RouteBackend {
				weight: 1,
				port: dst.port(), // TODO: get from req
				backend: Backend::Service(svc.namespaced_hostname()),
				filters: Vec::new(),
			}],
		};
		// If there is no route, use a default one
		return Some((
			Arc::new(default_route),
			PathMatch::PathPrefix(strng::new("/")),
		));
	}
	listener
		.routes
		.iter()
		// First, make sure the hostname matches and get it's rank
		.filter_map(|r| {
			// (route, (exact, match len))
			if r.hostnames.is_empty() {
				return Some((r, (false, 0)));
			}
			if let Some(best) = r.hostnames.iter().find(|h| h.as_str() == host) {
				let bl = best.len();
				return Some((r, (true, bl)));
			}
			if let Some(best) = r
				.hostnames
				.iter()
				.sorted_by_key(|h| -(h.len() as i64))
				.find(|h| h.starts_with("*") && host.ends_with(&h.as_str()[1..]))
			{
				let bl = best.len();
				return Some((r, (false, bl)));
			}
			None
		})
		// .sorted_by_key(|(_, score)| score.clone()) // not needed
		.filter_map(|(r, hostname_score)| {
			// Now rank the rest...
			let scores = r
				.matches
				.iter()
				.filter_map(|m| {
					// MISSING: creation timestamp
					// (hostname score, is exact, prefix len, has method, header count, query count, route name)
					// Note: name should be {namespace}/{name} but we really get namespace.name.idx.idx from control plane (and its opaque)
					// We want lowest namespace/name, and the lowest route number
					let mut score = (
						hostname_score,
						false,
						0,
						false,
						0,
						0,
						std::cmp::Reverse(r.name.clone()),
					);
					let path_matches = match &m.path {
						PathMatch::Exact(p) => {
							score.1 = true;
							request.uri().path() == p.as_str()
						},
						PathMatch::Regex(r, rlen) => {
							score.2 = *rlen;
							let path = request.uri().path();
							r.find(path)
								.map(|m| m.start() == 0 && m.end() == path.len())
								.unwrap_or(false)
						},
						PathMatch::PathPrefix(p) => {
							score.2 = p.len();
							let p = p.trim_end_matches('/');
							let suffix = request.uri().path().trim_end_matches('/').strip_prefix(p)?;
							// TODO this is not right!!
							suffix.is_empty() || suffix.starts_with('/')
						},
					};
					// TODO the rest
					if !path_matches {
						return None;
					}

					if let Some(method) = &m.method {
						if request.method().as_str() != method.method.as_str() {
							return None;
						}
						score.3 = true;
					}
					for HeaderMatch { name, value } in &m.headers {
						let have = request.headers().get(name.as_str())?;
						match value {
							HeaderValueMatch::Exact(want) => {
								if have != want {
									return None;
								}
							},
							HeaderValueMatch::Regex(want) => {
								// Must be a valid string to do regex match
								let have = have.to_str().ok()?;
								let m = want.find(have)?;
								// Make sure we matched the entire thing
								if !(m.start() == 0 && m.end() == have.len()) {
									return None;
								}
							},
						}
					}
					score.4 = m.headers.len();
					let query = request
						.uri()
						.query()
						.map(|q| url::form_urlencoded::parse(q.as_bytes()).collect::<HashMap<_, _>>())
						.unwrap_or_default();
					for agent::QueryMatch { name, value } in &m.query {
						let have = query.get(name.as_str())?;

						match value {
							QueryValueMatch::Exact(want) => {
								if have.as_ref() != want.as_str() {
									return None;
								}
							},
							QueryValueMatch::Regex(want) => {
								// Must be a valid string to do regex match
								let m = want.find(have)?;
								// Make sure we matched the entire thing
								if !(m.start() == 0 && m.end() == have.len()) {
									return None;
								}
							},
						}
					}
					score.5 = m.query.len();
					Some((m.path.clone(), score))
				})
				.collect_vec();
			tracing::trace!("howardjohn: route scores for {}: {:?}", r.name, scores);
			scores
				.into_iter()
				.max_by_key(|(_, score)| score.clone())
				.map(|(mtch, score)| (r, mtch, score))
		})
		.max_by_key(|(_r, _mtch, scores)| scores.clone())
		.map(|(r, mtch, _)| (r, mtch))
}

mod revproxy {
	use crate::http::Body;
	use http::Uri;
	use http::uri::PathAndQuery;
	use http_body_util::Empty;
	use hyper::header::{HeaderMap, HeaderName, HeaderValue};
	use hyper::http::header::{InvalidHeaderValue, ToStrError};
	use hyper::http::uri::InvalidUri;
	use hyper::{Error, Request, Response, StatusCode};
	use hyper_util::client::legacy::{Client, Error as LegacyError, connect::Connect};
	use hyper_util::rt::tokio::TokioIo;
	use std::net::{IpAddr, SocketAddr};
	use std::sync::OnceLock;
	use tokio::io::copy_bidirectional;
	use tracing::{debug, warn};

	fn te_header() -> &'static HeaderName {
		static TE_HEADER: OnceLock<HeaderName> = OnceLock::new();
		TE_HEADER.get_or_init(|| HeaderName::from_static("te"))
	}

	fn connection_header() -> &'static HeaderName {
		static CONNECTION_HEADER: OnceLock<HeaderName> = OnceLock::new();
		CONNECTION_HEADER.get_or_init(|| HeaderName::from_static("connection"))
	}

	fn upgrade_header() -> &'static HeaderName {
		static UPGRADE_HEADER: OnceLock<HeaderName> = OnceLock::new();
		UPGRADE_HEADER.get_or_init(|| HeaderName::from_static("upgrade"))
	}

	fn trailer_header() -> &'static HeaderName {
		static TRAILER_HEADER: OnceLock<HeaderName> = OnceLock::new();
		TRAILER_HEADER.get_or_init(|| HeaderName::from_static("trailer"))
	}

	fn trailers_header() -> &'static HeaderName {
		static TRAILERS_HEADER: OnceLock<HeaderName> = OnceLock::new();
		TRAILERS_HEADER.get_or_init(|| HeaderName::from_static("trailers"))
	}

	fn x_forwarded_for_header() -> &'static HeaderName {
		static X_FORWARDED_FOR: OnceLock<HeaderName> = OnceLock::new();
		X_FORWARDED_FOR.get_or_init(|| HeaderName::from_static("x-forwarded-for"))
	}

	fn hop_headers() -> &'static [HeaderName; 9] {
		static HOP_HEADERS: OnceLock<[HeaderName; 9]> = OnceLock::new();
		HOP_HEADERS.get_or_init(|| {
			[
				connection_header().clone(),
				te_header().clone(),
				trailer_header().clone(),
				HeaderName::from_static("keep-alive"),
				HeaderName::from_static("proxy-connection"),
				HeaderName::from_static("proxy-authenticate"),
				HeaderName::from_static("proxy-authorization"),
				HeaderName::from_static("transfer-encoding"),
				HeaderName::from_static("upgrade"),
			]
		})
	}

	#[derive(Debug, thiserror::Error)]
	pub enum ProxyError {
		#[error("InvalidUri: {0}")]
		InvalidUri(InvalidUri),
		#[error("LegacyHyperError: {0}")]
		LegacyHyperError(LegacyError),
		#[error("HyperError: {0}")]
		HyperError(Error),
		#[error("ForwardHeaderError")]
		ForwardHeaderError,
		#[error("UpgradeError: {0}")]
		UpgradeError(String),
		#[error("UpstreamError: {0}")]
		UpstreamError(String),
	}

	impl From<LegacyError> for ProxyError {
		fn from(err: LegacyError) -> ProxyError {
			ProxyError::LegacyHyperError(err)
		}
	}

	impl From<Error> for ProxyError {
		fn from(err: Error) -> ProxyError {
			ProxyError::HyperError(err)
		}
	}

	impl From<InvalidUri> for ProxyError {
		fn from(err: InvalidUri) -> ProxyError {
			ProxyError::InvalidUri(err)
		}
	}

	impl From<ToStrError> for ProxyError {
		fn from(_err: ToStrError) -> ProxyError {
			ProxyError::ForwardHeaderError
		}
	}

	impl From<InvalidHeaderValue> for ProxyError {
		fn from(_err: InvalidHeaderValue) -> ProxyError {
			ProxyError::ForwardHeaderError
		}
	}

	fn remove_hop_headers(headers: &mut HeaderMap) {
		debug!("Removing hop headers");

		for header in hop_headers() {
			headers.remove(header);
		}
	}

	fn get_upgrade_type(headers: &HeaderMap) -> Option<String> {
		#[allow(clippy::blocks_in_conditions)]
		if headers
			.get(connection_header())
			.map(|value| {
				value
					.to_str()
					.unwrap()
					.split(',')
					.any(|e| e.trim() == *upgrade_header())
			})
			.unwrap_or(false)
		{
			if let Some(upgrade_value) = headers.get(upgrade_header()) {
				debug!(
					"Found upgrade header with value: {}",
					upgrade_value.to_str().unwrap().to_owned()
				);

				return Some(upgrade_value.to_str().unwrap().to_owned());
			}
		}

		None
	}

	fn remove_connection_headers(headers: &mut HeaderMap) {
		if headers.get(connection_header()).is_some() {
			debug!("Removing connection headers");

			let value = headers.get(connection_header()).cloned().unwrap();

			for name in value.to_str().unwrap().split(',') {
				if !name.trim().is_empty() {
					headers.remove(name.trim());
				}
			}
		}
	}

	fn create_proxied_response<B>(mut response: Response<B>) -> Response<B> {
		debug!("Creating proxied response");

		remove_hop_headers(response.headers_mut());
		remove_connection_headers(response.headers_mut());

		response
	}

	fn create_forward_uri<B>(forward_url: &str, req: &Request<B>) -> Uri {
		Uri::builder()
			.scheme("http")
			.authority(forward_url)
			.path_and_query(
				req
					.uri()
					.path_and_query()
					.unwrap_or(&PathAndQuery::from_static("/"))
					.clone(),
			)
			.build()
			.expect("TODO")
		/*
		debug!("Building forward uri");

		let split_url = forward_url.split('?').collect::<Vec<&str>>();

		let mut base_url: &str = split_url.first().unwrap_or(&"");
		let forward_url_query: &str = split_url.get(1).unwrap_or(&"");

		let path2 = req.uri().path();

		if base_url.ends_with('/') {
			let mut path1_chars = base_url.chars();
			path1_chars.next_back();

			base_url = path1_chars.as_str();
		}

		let total_length = base_url.len()
			+ path2.len()
			+ 1
			+ forward_url_query.len()
			+ req.uri().query().map(|e| e.len()).unwrap_or(0);

		debug!("Creating url with capacity to {}", total_length);

		let mut url = String::with_capacity(total_length);

		url.push_str(base_url);
		url.push_str(path2);

		if !forward_url_query.is_empty() || req.uri().query().map(|e| !e.is_empty()).unwrap_or(false) {
			debug!("Adding query parts to url");
			url.push('?');
			url.push_str(forward_url_query);

			if forward_url_query.is_empty() {
			debug!("Using request query");

			url.push_str(req.uri().query().unwrap_or(""));
			} else {
			debug!("Merging request and forward_url query");

			let request_query_items = req.uri().query().unwrap_or("").split('&').map(|el| {
			let parts = el.split('=').collect::<Vec<&str>>();
			(parts[0], if parts.len() > 1 { parts[1] } else { "" })
			});

			let forward_query_items = forward_url_query
			.split('&')
			.map(|el| {
			let parts = el.split('=').collect::<Vec<&str>>();
			parts[0]
			})
			.collect::<Vec<_>>();

			for (key, value) in request_query_items {
			if !forward_query_items.iter().any(|e| e == &key) {
			url.push('&');
			url.push_str(key);
			url.push('=');
			url.push_str(value);
			}
			}

			if url.ends_with('&') {
			let mut parts = url.chars();
			parts.next_back();

			url = parts.as_str().to_string();
			}
			}
		}

		debug!("Built forwarding url from request: {}", url);

		url.parse().unwrap()

		 */
	}

	fn create_proxied_request<B>(
		client_ip: IpAddr,
		mut request: Request<B>,
		upgrade_type: Option<&String>,
	) -> Result<Request<B>, ProxyError> {
		debug!("Creating proxied request");

		let contains_te_trailers_value = request
			.headers()
			.get(te_header())
			.map(|value| {
				value
					.to_str()
					.unwrap()
					.split(',')
					.any(|e| e.trim() == *trailers_header())
			})
			.unwrap_or(false);

		debug!("Setting headers of proxied request");

		remove_hop_headers(request.headers_mut());
		remove_connection_headers(request.headers_mut());

		if contains_te_trailers_value {
			debug!("Setting up trailer headers");

			request
				.headers_mut()
				.insert(te_header(), HeaderValue::from_static("trailers"));
		}

		if let Some(value) = upgrade_type {
			debug!("Repopulate upgrade headers");

			request
				.headers_mut()
				.insert(upgrade_header(), value.parse().unwrap());
			request
				.headers_mut()
				.insert(connection_header(), HeaderValue::from_static("UPGRADE"));
		}

		// Add forwarding information in the headers
		match request.headers_mut().entry(x_forwarded_for_header()) {
			hyper::header::Entry::Vacant(entry) => {
				debug!("X-Forwarded-for header was vacant");
				entry.insert(client_ip.to_string().parse()?);
			},

			hyper::header::Entry::Occupied(entry) => {
				debug!("X-Forwarded-for header was occupied");
				let client_ip_str = client_ip.to_string();
				let mut addr =
					String::with_capacity(entry.get().as_bytes().len() + 2 + client_ip_str.len());

				addr.push_str(std::str::from_utf8(entry.get().as_bytes()).unwrap());
				addr.push(',');
				addr.push(' ');
				addr.push_str(&client_ip_str);
			},
		}

		debug!("Created proxied request");

		Ok(request)
	}

	fn get_upstream_addr(forward_uri: &str) -> Result<SocketAddr, ProxyError> {
		let forward_uri: hyper::Uri = forward_uri.parse().map_err(|e| {
			ProxyError::UpstreamError(format!("parsing forward_uri as a Uri: {e}").to_string())
		})?;
		let host = forward_uri.host().ok_or(ProxyError::UpstreamError(
			"forward_uri has no host".to_string(),
		))?;
		let port = forward_uri.port_u16().ok_or(ProxyError::UpstreamError(
			"forward_uri has no port".to_string(),
		))?;
		format!("{host}:{port}")
			.parse()
			.map_err(|_| ProxyError::UpstreamError("forward_uri host must be an IP address".to_string()))
	}

	type ResponseBody = Body;

	pub async fn call<T: Connect + Clone + Send + Sync + 'static>(
		client_ip: IpAddr,
		forward_uri: &str,
		request: Request<Body>,
		h1_client: &Client<T, Body>,
		h2_client: &Client<T, Body>,
	) -> Result<Response<ResponseBody>, ProxyError> {
		debug!(
			"Received proxy call from {} to {}, client: {}",
			request.uri().to_string(),
			forward_uri,
			client_ip
		);

		let request_upgrade_type = get_upgrade_type(request.headers());

		let mut request = create_proxied_request(client_ip, request, request_upgrade_type.as_ref())?;

		if request_upgrade_type.is_none() {
			let request_uri: hyper::Uri = create_forward_uri(forward_uri, &request);
			*request.uri_mut() = request_uri.clone();

			let client = if request.version() == http::Version::HTTP_2 {
				h2_client
			} else {
				h1_client
			};
			let response = client.request(request).await?;

			debug!("Responding to call with response");
			return Ok(create_proxied_response(response.map(Body::new)));
		}

		let upstream_addr = get_upstream_addr(forward_uri)?;
		let (request_parts, request_body) = request.into_parts();
		let upstream_request =
			Request::from_parts(request_parts.clone(), Empty::<hyper::body::Bytes>::new());
		let mut downstream_request = Request::from_parts(request_parts, request_body);

		let (mut upstream_conn, downstream_response) = {
			let conn = TokioIo::new(
				tokio::net::TcpStream::connect(upstream_addr)
					.await
					.map_err(|e| ProxyError::UpstreamError(e.to_string()))?,
			);
			let (mut sender, conn) = hyper::client::conn::http1::handshake(conn).await?;

			tokio::task::spawn(async move {
				if let Err(err) = conn.with_upgrades().await {
					warn!("Upgrading connection failed: {:?}", err);
				}
			});

			let response = sender.send_request(upstream_request).await?;

			if response.status() != StatusCode::SWITCHING_PROTOCOLS {
				return Err(ProxyError::UpgradeError(
					"Server did not response with Switching Protocols status".to_string(),
				));
			};

			let (response_parts, response_body) = response.into_parts();
			let upstream_response = Response::from_parts(response_parts.clone(), response_body);
			let downstream_response = Response::from_parts(response_parts, Empty::new());

			(
				TokioIo::new(hyper::upgrade::on(upstream_response).await?),
				downstream_response,
			)
		};

		tokio::task::spawn(async move {
			let mut downstream_conn = match hyper::upgrade::on(&mut downstream_request).await {
				Ok(upgraded) => TokioIo::new(upgraded),
				Err(e) => {
					warn!("Failed to upgrade downstream request: {e}");
					return;
				},
			};

			if let Err(e) = copy_bidirectional(&mut downstream_conn, &mut upstream_conn).await {
				warn!("Bidirectional copy failed: {e}");
			}
		});

		Ok(downstream_response.map(Body::new))
	}

	#[derive(Debug, Clone)]
	pub struct ReverseProxy<T: Connect + Clone + Send + Sync + 'static> {
		h1_client: Client<T, Body>,
		h2_client: Client<T, Body>,
	}

	impl<T: Connect + Clone + Send + Sync + 'static> ReverseProxy<T> {
		pub fn new(h1_client: Client<T, Body>, h2_client: Client<T, Body>) -> Self {
			// Im sure there is a way to do this with 1 client but https://stackoverflow.com/questions/71040835/how-to-allow-both-http1-and-http2-requests-in-hyper
			Self {
				h1_client,
				h2_client,
			}
		}

		pub async fn call(
			&self,
			client_ip: IpAddr,
			forward_uri: &str,
			request: Request<Body>,
		) -> Result<Response<ResponseBody>, ProxyError> {
			call::<T>(
				client_ip,
				forward_uri,
				request,
				&self.h1_client,
				&self.h2_client,
			)
			.await
		}
	}
}

fn load_balance(
	pi: Arc<ProxyInputs>,
	svc: &Service,
	svc_port: u16,
	override_dest: Option<SocketAddr>,
) -> Option<(&Endpoint, Arc<Workload>)> {
	let state = &pi.store;
	let workloads = &state.read_discovery().workloads;
	let target_port = svc.ports.get(&svc_port).copied();

	if target_port.is_none() {
		// Port doesn't exist on the service at all, this is invalid
		debug!("service {} does not have port {}", svc.hostname, svc_port);
		return None;
	};

	let endpoints = svc.endpoints.iter().filter_map(|ep| {
		let Some(wl) = workloads.find_uid(&ep.workload_uid) else {
			debug!("failed to fetch workload for {}", ep.workload_uid);
			return None;
		};
		if target_port.unwrap_or_default() == 0 && !ep.port.contains_key(&svc_port) {
			// Filter workload out, it doesn't have a matching port
			trace!(
				"filter endpoint {}, it does not have service port {}",
				ep.workload_uid, svc_port
			);
			return None;
		}
		if let Some(o) = override_dest {
			if !wl.workload_ips.contains(&o.ip()) {
				// We ignore port, assume its a bug to have a mismatch
				trace!(
					"filter endpoint {}, it was not the selected endpoint {}",
					ep.workload_uid, o
				);
				return None;
			}
		}
		Some((ep, wl))
	});

	let options = endpoints.collect_vec();
	options
		.choose_weighted(&mut rand::rng(), |(_, wl)| wl.capacity as u64)
		// This can fail if there are no weights, the sum is zero (not possible in our API), or if it overflows
		// The API has u32 but we sum into an u64, so it would take ~4 billion entries of max weight to overflow
		.ok()
		.cloned()
}
fn strip_port(auth: &str) -> &str {
	let host_port = auth
		.rsplit('@')
		.next()
		.expect("split always has at least 1 item");

	if host_port.as_bytes()[0] == b'[' {
		let i = host_port
			.find(']')
			.expect("parsing should validate brackets");
		// ..= ranges aren't available in 1.20, our minimum Rust version...
		&host_port[0..i + 1]
	} else {
		host_port
			.split(':')
			.next()
			.expect("split always has at least 1 item")
	}
}

// Protocol of the entire bind. TODO: we should make this a property of the API
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum BindProtocol {
	Http,
	Https,
	Hbone,
}

fn bind_protocol(inp: Arc<ProxyInputs>, bind: BindName) -> BindProtocol {
	let listeners = inp.store.read_binds().listeners(bind).unwrap();
	if listeners
		.iter()
		.any(|l| matches!(l.protocol, ListenerProtocol::HBONE))
	{
		return BindProtocol::Hbone;
	}
	if listeners.iter().any(|l| l.protocol.tls().is_some()) {
		return BindProtocol::Https;
	}
	BindProtocol::Http
}

fn build_response(status: StatusCode) -> ::http::Response<()> {
	::http::Response::builder()
		.status(status)
		.body(())
		.expect("builder with known status code should not fail")
}

/// InboundError represents an error with an associated status code.
#[derive(Debug)]
struct InboundError(anyhow::Error, StatusCode);
impl InboundError {
	pub fn build(code: StatusCode) -> impl Fn(anyhow::Error) -> Self {
		move |err| InboundError(err, code)
	}
}

#[derive(Clone)]
struct HTTPProxy {
	upstream: Upstream,
	bind_name: BindName,
	inputs: Arc<ProxyInputs>,
	selected_listener: Option<Arc<Listener>>,
	target_address: SocketAddr,
}

#[derive(thiserror::Error, Debug)]
enum ProxyError {
	#[error("bind not found")]
	BindNotFound,
	#[error("listener not found")]
	ListenerNotFound,
	#[error("route not found")]
	RouteNotFound,
	#[error("no valid backends")]
	NoValidBackends,
	#[error("backends does not exist")]
	BackendDoesNotExist,
	#[error("service not found")]
	ServiceNotFound,
	#[error("no healthy backends")]
	NoHealthyEndpoints,
	#[error("upstream call failed: {0:?}")]
	UpstreamCallFailed(revproxy::ProxyError),
	#[error("request timeout")]
	RequestTimeout,
	#[error("processing failed: {0:?}")]
	Processing(anyhow::Error),
}

impl ProxyError {
	pub fn as_response(&self) -> Response {
		let code = match self {
			ProxyError::BindNotFound => StatusCode::NOT_FOUND,
			ProxyError::ListenerNotFound => StatusCode::NOT_FOUND,
			ProxyError::RouteNotFound => StatusCode::NOT_FOUND,
			ProxyError::NoValidBackends => StatusCode::INTERNAL_SERVER_ERROR,
			ProxyError::BackendDoesNotExist => StatusCode::INTERNAL_SERVER_ERROR,
			ProxyError::ServiceNotFound => StatusCode::INTERNAL_SERVER_ERROR,
			ProxyError::NoHealthyEndpoints => StatusCode::SERVICE_UNAVAILABLE,
			ProxyError::UpstreamCallFailed(_) => StatusCode::SERVICE_UNAVAILABLE,
			ProxyError::RequestTimeout => StatusCode::GATEWAY_TIMEOUT,
			ProxyError::Processing(_) => StatusCode::SERVICE_UNAVAILABLE,
		};
		let msg = self.to_string();
		::http::Response::builder()
			.status(code)
			.header(hyper::header::CONTENT_TYPE, "text/plain")
			.body(http::Body::from(msg))
			.unwrap()
	}
}

#[derive(Debug)]
struct RequestLog {
	bind: SocketAddr,
	listener_name: ListenerGroupName,
	route_name: Strng,
	host: String,
	backend: String,
	endpoint: String,
}

impl Drop for RequestLog {
	fn drop(&mut self) {
		event!(
			target: "request",
			parent: None,
			tracing::Level::INFO,

			bind.name = %self.bind,
			listener.name = %self.listener_name,
			route.name = %self.route_name,

			backend = %self.backend,
			endpoint = %self.endpoint,

			host = %self.host,
		);
	}
}

impl HTTPProxy {
	pub async fn proxy(
		&self,
		connection: Arc<Extension>,
		req: ::http::Request<Incoming>,
	) -> Response {
		self
			.proxy_internal(connection, req)
			.await
			.unwrap_or_else(|err| err.as_response())
	}
	async fn proxy_internal(
		&self,
		connection: Arc<Extension>,
		req: ::http::Request<Incoming>,
	) -> Result<Response, ProxyError> {
		let tcp = connection.get::<TcpConnectionInfo>().unwrap();
		let mut log = RequestLog {
			bind: tcp.local_addr,
			listener_name: Default::default(),
			route_name: Default::default(),
			host: Default::default(),
			backend: Default::default(),
			endpoint: Default::default(),
		};
		let client_ip: IpAddr = "127.0.0.1".parse().expect("TODO");
		let selected_listener = self.selected_listener.clone();
		let upstream = self.upstream.clone();
		let inputs = self.inputs.clone();
		let bind_name = self.bind_name.clone();
		debug!(bind=%bind_name, "route for bind");
		let Some(listeners) = ({
			let state = inputs.store.read_binds();
			state.listeners(bind_name.clone())
		}) else {
			return Err(ProxyError::BindNotFound);
		};

		let mut req = req.map(http::Body::new);

		normalize_uri(&connection, &mut req).map_err(ProxyError::Processing)?;
		let host = get_host(&req);
		log.host = host.to_string();

		let selected_listener = selected_listener
			.or_else(|| listeners.best_match(host))
			.ok_or(ProxyError::ListenerNotFound)?;
		log.listener_name = selected_listener.group_name.clone();

		debug!(bind=%bind_name, listener=%selected_listener.name, "selected listener");

		let (selected_route, path_match) = select_best_route(
			inputs.clone(),
			self.target_address,
			selected_listener.clone(),
			&req,
		)
		.ok_or(ProxyError::RouteNotFound)?;
		log.route_name = selected_route.group_name.clone();

		debug!(bind=%bind_name, listener=%selected_listener.name, route=%selected_route.name, "selected route");
		if let Some(resp) = apply_request_filters(
			selected_route.as_ref().filters.as_slice(),
			&path_match,
			&mut req,
		) {
			return Ok(resp);
		}
		let mut mirrors = get_mirrors(selected_route.as_ref().filters.as_slice());
		let selected_backend =
			select_backend(selected_route.as_ref(), &req).ok_or(ProxyError::NoValidBackends)?;
		if let Some(resp) =
			apply_request_filters(selected_backend.filters.as_slice(), &path_match, &mut req)
		{
			return Ok(resp);
		}
		mirrors.extend(get_mirrors(selected_backend.filters.as_slice()));
		let (head, body) = req.into_parts();
		for mirror in mirrors {
			if !rand::rng().random_bool(mirror.percentage) {
				trace!(
					"skipping mirror, percentage {} not triggered",
					mirror.percentage
				);
				continue;
			}
			// TODO: do we need to mirror the body?
			let req = Request::from_parts(head.clone(), http::Body::empty());
			let upstream = self.upstream.clone();
			let inputs = inputs.clone();
			tokio::task::spawn(async move {
				if let Err(e) = send_mirror(inputs, upstream, mirror, req).await {
					warn!("error sending mirror request: {}", e);
				}
			});
		}
		let mut req = Request::from_parts(head, http::Body::new(body));
		const INFERENCE: bool = false; // TODO: xds
		let override_dest: Option<SocketAddr> = if INFERENCE {
			let mut ext_proc = ExtProc::new().await.unwrap();
			req = ext_proc.request_headers(&mut req).await;
			req
				.headers()
				.get(HeaderName::from_static("X-Gateway-Destination-Endpoint"))
				.and_then(|v| v.to_str().ok())
				.map(|v| v.parse::<SocketAddr>().expect("TODO"))
		} else {
			None
		};
		let dest = match &selected_backend.backend {
			Backend::Service(svc_key) => {
				let svc = inputs
					.store
					.read_discovery()
					.services
					.get_by_namespaced_host(svc_key)
					.ok_or(ProxyError::ServiceNotFound)?;
				let (ep, wl) = load_balance(
					inputs.clone(),
					svc.as_ref(),
					selected_backend.port,
					override_dest,
				)
				.ok_or(ProxyError::NoHealthyEndpoints)?;
				let svc_target_port = svc
					.ports
					.get(&selected_backend.port)
					.copied()
					.unwrap_or_default();
				let target_port = if let Some(&ep_target_port) = ep.port.get(&selected_backend.port) {
					// prefer endpoint port mapping
					ep_target_port
				} else if svc_target_port > 0 {
					// otherwise, see if the service has this port
					svc_target_port
				} else {
					return Err(ProxyError::NoHealthyEndpoints);
				};
				if svc.port_is_http2(selected_backend.port) {
					req.headers_mut().remove(http::header::TRANSFER_ENCODING);
					*req.version_mut() = ::http::Version::HTTP_2;
				}
				log.backend = format!("{}/{}:{}", svc.namespace, svc.name, target_port);
				SocketAddr::from((*wl.workload_ips.first().expect("TODO"), target_port))
			},
			Backend::Opaque(destination) => {
				// Always use HTTP, they can explicitly configure for HTTP2 (in future)
				SocketAddr::from((*destination, selected_backend.port))
			},
			Backend::Invalid => {
				return Err(ProxyError::BackendDoesNotExist);
			},
		};

		let timeout = match &selected_route.policies {
			Some(TrafficPolicy { timeout }) => timeout.effective_timeout(),
			_ => None,
		};
		let fwd = format!("{dest}");
		// let fwd = "127.0.0.1:8081".to_string();
		let (call, body_timeout) = if let Some(timeout) = timeout {
			let deadline = tokio::time::Instant::from_std(
				connection.get::<TcpConnectionInfo>().unwrap().start + timeout,
			);
			let fut = tokio::time::timeout_at(deadline, upstream.client.call(client_ip, &fwd, req));
			(fut, http::timeout::BodyTimeout::Deadline(deadline))
		} else {
			let fut = tokio::time::timeout(Duration::MAX, upstream.client.call(client_ip, &fwd, req));
			(fut, http::timeout::BodyTimeout::Duration(Duration::MAX))
		};
		log.endpoint = fwd.clone();
		let mut resp = match call.await {
			Ok(Ok(resp)) => resp,
			Ok(Err(e)) => {
				return Err(ProxyError::UpstreamCallFailed(e));
			},
			Err(_) => {
				return Err(ProxyError::RequestTimeout);
			},
		};
		if false {
			// do PII here
		}
		apply_response_filters(selected_route.filters.as_slice(), &mut resp);
		apply_response_filters(selected_backend.filters.as_slice(), &mut resp);
		let resp = body_timeout.apply(resp);
		Ok(resp)
	}
}

async fn send_mirror(
	inputs: Arc<ProxyInputs>,
	upstream: Upstream,
	mirror: filters::RequestMirror,
	mut req: Request,
) -> Result<(), ProxyError> {
	let client_ip: IpAddr = "127.0.0.1".parse().expect("TODO");
	let dest = match &mirror.backend {
		Backend::Service(svc_key) => {
			let svc = inputs
				.store
				.read_discovery()
				.services
				.get_by_namespaced_host(svc_key)
				.ok_or(ProxyError::ServiceNotFound)?;
			let (ep, wl) = load_balance(inputs.clone(), svc.as_ref(), mirror.port, None)
				.ok_or(ProxyError::NoHealthyEndpoints)?;
			let svc_target_port = svc.ports.get(&mirror.port).copied().unwrap_or_default();
			let target_port = if let Some(&ep_target_port) = ep.port.get(&mirror.port) {
				// prefer endpoint port mapping
				ep_target_port
			} else if svc_target_port > 0 {
				// otherwise, see if the service has this port
				svc_target_port
			} else {
				return Err(ProxyError::NoHealthyEndpoints);
			};
			if svc.port_is_http2(mirror.port) {
				req.headers_mut().remove(http::header::TRANSFER_ENCODING);
				*req.version_mut() = ::http::Version::HTTP_2;
			}
			SocketAddr::from((*wl.workload_ips.first().expect("TODO"), target_port))
		},
		Backend::Opaque(destination) => {
			// Always use HTTP, they can explicitly configure for HTTP2 (in future)
			SocketAddr::from((*destination, mirror.port))
		},
		Backend::Invalid => {
			return Err(ProxyError::BackendDoesNotExist);
		},
	};
	let fwd = format!("{dest}");
	upstream
		.client
		.call(client_ip, &fwd, req)
		.await
		.map_err(ProxyError::UpstreamCallFailed)?;
	Ok(())
}

// The http library will not put the authority into req.uri().authority for HTTP/1. Normalize so
// the rest of the code doesn't need to worry about it
fn normalize_uri(connection: &Extension, req: &mut Request) -> anyhow::Result<()> {
	debug!("request before normalization: {req:?}");
	if let ::http::Version::HTTP_10 | ::http::Version::HTTP_11 = req.version() {
		if req.uri().authority().is_none() {
			let mut parts = std::mem::take(req.uri_mut()).into_parts();
			// TODO: handle absolute HTTP/1.1 form
			let host = req
				.headers()
				.get(http::header::HOST)
				.and_then(|h| h.to_str().ok())
				.and_then(|h| h.parse::<Authority>().ok())
				.ok_or_else(|| anyhow::anyhow!("no authority or host"))?;

			parts.authority = Some(host);
			if parts.path_and_query.is_some() {
				// TODO: or always do this?
				if connection.get::<TLSConnectionInfo>().is_some() {
					parts.scheme = Some(Scheme::HTTPS);
				} else {
					parts.scheme = Some(Scheme::HTTP);
				}
			}
			*req.uri_mut() = Uri::from_parts(parts)?
		}
	}
	debug!("request after normalization: {req:?}");
	Ok(())
}

fn get_host(req: &Request) -> &str {
	// HTTP2 will be in URI, HTTP/1.1 will be in header
	// TODO: handle absolute HTTP/1.1 form
	let host = req
		.uri()
		.host()
		.or_else(|| {
			req
				.headers()
				.get(http::header::HOST)
				.and_then(|h| h.to_str().ok())
		})
		.expect("TODO validate not fail");
	let host = strip_port(host);
	host
}

pub fn pooling_client<B>(
	connector: hbone::HBONEConnector,
) -> ::hyper_util::client::legacy::Client<hbone::HBONEConnector, B>
where
	B: http_body::Body + Send,
	B::Data: Send,
{
	::hyper_util::client::legacy::Client::builder(::hyper_util::rt::TokioExecutor::new())
		.timer(hyper_util::rt::tokio::TokioTimer::new())
		.build(connector)
}

pub fn pooling_h2_client<B>(
	connector: hbone::HBONEConnector,
) -> ::hyper_util::client::legacy::Client<hbone::HBONEConnector, B>
where
	B: http_body::Body + Send,
	B::Data: Send,
{
	::hyper_util::client::legacy::Client::builder(::hyper_util::rt::TokioExecutor::new())
		.timer(hyper_util::rt::tokio::TokioTimer::new())
		.http2_only(true)
		.build(connector)
}
pub fn auto_server() -> auto::Builder<::hyper_util::rt::TokioExecutor> {
	let mut b = auto::Builder::new(::hyper_util::rt::TokioExecutor::new());
	b.http2().timer(hyper_util::rt::tokio::TokioTimer::new());
	b
}
