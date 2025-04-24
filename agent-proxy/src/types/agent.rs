use crate::http::{HeaderName, HeaderValue, status};
use crate::http::{StatusCode, uri};
use crate::http::{filters, timeout};
// use crate::state::workload::{NamespacedHostname, ProtoError};
use crate::types::discovery::NamespacedHostname;
use crate::types::proto;
use crate::*;
use anyhow::anyhow;
use itertools::Itertools;
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls_pemfile::Item;
use serde::Serializer;
use std::collections::HashMap;
use std::fmt::Display;
use std::io::Cursor;
use std::net;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU16;
use std::sync::Arc;
use thiserror::Error;
use crate::types::proto::ProtoError;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Bind {
	pub name: BindName,
	pub address: SocketAddr,
	pub listeners: ListenerSet,
}

pub type BindName = Strng;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Listener {
	pub name: ListenerName,
	/// Can be a wildcard
	pub hostname: Strng,
	pub protocol: ListenerProtocol,
	pub group_name: ListenerGroupName,
	pub routes: RouteSet,
}

pub type ListenerGroupName = Strng;

#[derive(Debug, Clone)]
pub struct TLSConfig {
	pub config: Arc<ServerConfig>,
}

impl serde::Serialize for TLSConfig {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		// TODO: store raw pem
		serializer.serialize_none()
	}
}

fn parse_cert(mut cert: &[u8]) -> Result<Vec<CertificateDer<'static>>, anyhow::Error> {
	let mut reader = std::io::BufReader::new(Cursor::new(&mut cert));
	let parsed: Result<Vec<_>, _> = rustls_pemfile::read_all(&mut reader).collect();
	parsed?
		.into_iter()
		.map(|p| {
			let Item::X509Certificate(der) = p else {
				return Err(anyhow!("no certificate"));
			};
			Ok(der)
		})
		.collect::<Result<Vec<_>, _>>()
}

fn parse_key(mut key: &[u8]) -> Result<PrivateKeyDer<'static>, anyhow::Error> {
	let mut reader = std::io::BufReader::new(Cursor::new(&mut key));
	let parsed = rustls_pemfile::read_one(&mut reader)?;
	let parsed = parsed.ok_or_else(|| anyhow!("no key"))?;
	match parsed {
		Item::Pkcs8Key(c) => Ok(PrivateKeyDer::Pkcs8(c)),
		Item::Pkcs1Key(c) => Ok(PrivateKeyDer::Pkcs1(c)),
		_ => Err(anyhow!("unsupported key")),
	}
}
#[derive(Debug, Clone, serde::Serialize)]
pub enum ListenerProtocol {
	HTTP,
	HTTPS(TLSConfig),
	TLS(TLSConfig),
	TCP,
	HBONE,
}

impl ListenerProtocol {
	pub fn tls(&self) -> Option<Arc<rustls::ServerConfig>> {
		match self {
			ListenerProtocol::HTTPS(t) | ListenerProtocol::TLS(t) => Some(t.config.clone()),
			_ => None,
		}
	}
}

pub type ListenerName = Strng;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Route {
	pub name: RouteName,
	pub group_name: RouteGroupName,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub section_name: Option<RouteSectionName>,
	/// Can be a wildcard
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub hostnames: Vec<Strng>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub matches: Vec<RouteMatch>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub filters: Vec<RouteFilter>,
	pub backends: Vec<RouteBackend>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub policies: Option<TrafficPolicy>,
}

pub type RouteName = Strng;
pub type RouteGroupName = Strng;
pub type RouteSectionName = Strng;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteMatch {
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub headers: Vec<HeaderMatch>,
	pub path: PathMatch,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub method: Option<MethodMatch>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub query: Vec<QueryMatch>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MethodMatch {
	pub method: Strng,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderMatch {
	#[serde(serialize_with = "serialize_display")]
	pub name: HeaderName,
	pub value: HeaderValueMatch,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryMatch {
	#[serde(serialize_with = "serialize_display")]
	pub name: Strng,
	pub value: QueryValueMatch,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum QueryValueMatch {
	Exact(Strng),
	Regex(#[serde(with = "serde_regex")] regex::Regex),
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum HeaderValueMatch {
	Exact(#[serde(serialize_with = "serialize_bytes")] HeaderValue),
	Regex(#[serde(with = "serde_regex")] regex::Regex),
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum PathMatch {
	Exact(Strng),
	PathPrefix(Strng),
	Regex(#[serde(with = "serde_regex")] regex::Regex, usize),
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum RouteFilter {
	RequestHeaderModifier(filters::HeaderModifier),
	ResponseHeaderModifier(filters::HeaderModifier),
	RequestRedirect(filters::RequestRedirect),
	UrlRewrite(filters::UrlRewrite),
	RequestMirror(filters::RequestMirror),
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrafficPolicy {
	pub timeout: timeout::Policy,
}

fn serialize_display<S: Serializer, T: Display>(t: &T, serializer: S) -> Result<S::Ok, S::Error> {
	serializer.serialize_str(&t.to_string())
}

fn serialize_bytes<S: Serializer, T: AsRef<[u8]>>(t: &T, serializer: S) -> Result<S::Ok, S::Error> {
	let b = t.as_ref();
	if let Ok(s) = std::str::from_utf8(b) {
		serializer.serialize_str(s)
	} else {
		serializer.serialize_str(&hex::encode(b))
	}
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum HostRedirect {
	Full(Strng),
	Host(Strng),
	Port(NonZeroU16),
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum PathRedirect {
	Full(Strng),
	Prefix(Strng),
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteBackend {
	pub weight: usize,
	pub port: u16,
	pub backend: Backend,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub filters: Vec<RouteFilter>,
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Backend {
	Service(NamespacedHostname),
	Opaque(IpAddr),
	Invalid,
}

#[derive(Debug, Clone, Default)]
pub struct ListenerSet {
	pub inner: HashMap<ListenerName, Arc<Listener>>,
}

impl serde::Serialize for ListenerSet {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		self.inner.serialize(serializer)
	}
}

impl ListenerSet {
	pub fn from_list<const N: usize>(l: [Listener; N]) -> ListenerSet {
		let mut listeners = HashMap::with_capacity(l.len());
		for ls in l.into_iter() {
			listeners.insert(ls.name.clone(), Arc::new(ls));
		}
		ListenerSet { inner: listeners }
	}

	pub fn best_match(&self, host: &str) -> Option<Arc<Listener>> {
		if let Some(best) = self.inner.values().find(|l| l.hostname == host) {
			trace!("found best match for {host} (exact)");
			return Some(best.clone());
		}
		if let Some(best) = self
			.inner
			.values()
			.sorted_by_key(|l| -(l.hostname.len() as i64))
			.find(|l| l.hostname.starts_with("*") && host.ends_with(&l.hostname.as_str()[1..]))
		{
			trace!("found best match for {host} (wildcard {})", best.hostname);
			return Some(best.clone());
		}
		trace!("trying to find best match for {host} (empty hostname)");
		self.inner.values().find(|l| l.hostname.is_empty()).cloned()
	}

	pub fn insert(&mut self, k: ListenerName, v: Listener) {
		self.inner.insert(k, Arc::new(v));
	}

	pub fn contains(&self, key: &ListenerName) -> bool {
		self.inner.contains_key(key)
	}

	pub fn get(&self, key: &ListenerName) -> Option<&Listener> {
		self.inner.get(key).map(Arc::as_ref)
	}

	pub fn remove(&mut self, key: &ListenerName) {
		self.inner.remove(key);
	}

	pub fn iter(&self) -> impl Iterator<Item = &Listener> {
		self.inner.values().map(Arc::as_ref)
	}
}

#[derive(Debug, Clone, Default)]
pub struct RouteSet {
	pub inner: HashMap<RouteName, Arc<Route>>,
}

impl serde::Serialize for RouteSet {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		self.inner.serialize(serializer)
	}
}

impl RouteSet {
	pub fn from_list<const N: usize>(l: [Route; N]) -> RouteSet {
		let mut routes = HashMap::with_capacity(l.len());
		for ls in l.into_iter() {
			routes.insert(ls.name.clone(), Arc::new(ls));
		}
		RouteSet { inner: routes }
	}

	pub fn insert(&mut self, k: RouteName, v: Route) {
		self.inner.insert(k, Arc::new(v));
	}

	pub fn contains(&self, key: &RouteName) -> bool {
		self.inner.contains_key(key)
	}

	pub fn get(&self, key: &RouteName) -> Option<&Route> {
		self.inner.get(key).map(Arc::as_ref)
	}

	pub fn remove(&mut self, key: &RouteName) {
		self.inner.remove(key);
	}

	pub fn iter(&self) -> impl Iterator<Item = Arc<Route>> {
		self.inner.values().map(Arc::clone)
	}

	pub fn is_empty(&self) -> bool {
		self.inner.is_empty()
	}
}

fn default_as_none<T: Default + PartialEq>(i: T) -> Option<T> {
	if i == Default::default() {
		None
	} else {
		Some(i)
	}
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, serde::Serialize)]
pub enum IpFamily {
	Dual,
	IPv4,
	IPv6,
}

impl TryFrom<&proto::adp::TlsConfig> for TLSConfig {
	type Error = anyhow::Error;

	fn try_from(value: &proto::adp::TlsConfig) -> Result<Self, Self::Error> {
		let cert_chain = parse_cert(&value.cert)?;
		let private_key = parse_key(&value.private_key)?;
		let mut sc = ServerConfig::builder_with_provider(transport::tls::provider())
			.with_protocol_versions(transport::tls::ALL_TLS_VERSIONS)
			.expect("server config must be valid")
			.with_no_client_auth()
			.with_single_cert(cert_chain, private_key)?;
		// TODO: support h2
		sc.alpn_protocols = vec![b"http/1.1".into()];
		Ok(TLSConfig {
			config: Arc::new(sc),
		})
	}
}

impl TryFrom<&proto::adp::RouteBackend> for RouteBackend {
	type Error = ProtoError;

	fn try_from(s: &proto::adp::RouteBackend) -> Result<Self, Self::Error> {
		let kind = match &s.kind {
			None => Backend::Invalid,
			Some(proto::adp::route_backend::Kind::Service(svc_key)) => {
				let ns = match svc_key.split_once('/') {
					Some((namespace, hostname)) => Ok(NamespacedHostname {
						namespace: namespace.into(),
						hostname: hostname.into(),
					}),
					None => Err(ProtoError::NamespacedHostnameParse(svc_key.clone())),
				}?;
				Backend::Service(ns)
			},
		};
		let filters = s
			.filters
			.iter()
			.map(RouteFilter::try_from)
			.collect::<Result<Vec<_>, _>>()?;
		Ok(Self {
			weight: s.weight as usize,
			port: s.port as u16,
			backend: kind,
			filters,
		})
	}
}

impl TryFrom<proto::adp::TrafficPolicy> for TrafficPolicy {
	type Error = ProtoError;

	fn try_from(s: proto::adp::TrafficPolicy) -> Result<Self, Self::Error> {
		let req = s.request_timeout.map(|v| v.try_into()).transpose()?;
		let backend = s
			.backend_request_timeout
			.map(|v| v.try_into())
			.transpose()?;

		Ok(Self {
			timeout: http::timeout::Policy {
				request_timeout: req,
				backend_request_timeout: backend,
			},
		})
	}
}

impl TryFrom<(proto::adp::Protocol, Option<&proto::adp::TlsConfig>)> for ListenerProtocol {
	type Error = ProtoError;
	fn try_from(
		value: (proto::adp::Protocol, Option<&proto::adp::TlsConfig>),
	) -> Result<Self, Self::Error> {
		use proto::adp::Protocol;
		match (value.0, value.1) {
			(Protocol::Unknown, _) => Err(ProtoError::EnumParse("unknown protocol".into())),
			(Protocol::Http, None) => Ok(ListenerProtocol::HTTP),
			(Protocol::Https, Some(tls)) => Ok(ListenerProtocol::HTTPS(
				tls
					.try_into()
					.map_err(|e| ProtoError::Generic(format!("{e}")))?,
			)),
			(Protocol::Tls, Some(tls)) => Ok(ListenerProtocol::TLS(
				tls
					.try_into()
					.map_err(|e| ProtoError::Generic(format!("{e}")))?,
			)),
			(Protocol::Tcp, None) => Ok(ListenerProtocol::TCP),
			(Protocol::Hbone, None) => Ok(ListenerProtocol::HBONE),
			(proto, tls) => Err(ProtoError::Generic(format!(
				"protocol {:?} is incompatible with {}",
				proto,
				if tls.is_some() {
					"tls"
				} else {
					"no tls config"
				}
			))),
		}
	}
}

impl TryFrom<&proto::adp::Bind> for Bind {
	type Error = ProtoError;

	fn try_from(s: &proto::adp::Bind) -> Result<Self, Self::Error> {
		Ok(Self {
			name: s.name.clone().into(),
			address: SocketAddr::from((IpAddr::from([0, 0, 0, 0]), s.port as u16)),
			listeners: Default::default(),
		})
	}
}

impl TryFrom<&proto::adp::Listener> for (Listener, BindName) {
	type Error = ProtoError;

	fn try_from(s: &proto::adp::Listener) -> Result<Self, Self::Error> {
		let proto = proto::adp::Protocol::try_from(s.protocol)?;
		let protocol = ListenerProtocol::try_from((proto, s.tls.as_ref()))
			.map_err(|e| ProtoError::Generic(format!("{e}")))?;
		let l = Listener {
			name: strng::new(&s.name),
			hostname: s.hostname.clone().into(),
			protocol,
			group_name: strng::new(&s.group),
			routes: Default::default(),
		};
		Ok((l, strng::new(&s.binding)))
	}
}

impl TryFrom<&proto::adp::Route> for (Route, ListenerName) {
	type Error = ProtoError;

	fn try_from(s: &proto::adp::Route) -> Result<Self, Self::Error> {
		let r = Route {
			name: strng::new(&s.name),
			group_name: strng::new(&s.group),
			section_name: default_as_none(s.section.as_str()).map(strng::new),
			hostnames: s.hostnames.iter().map(strng::new).collect(),
			// TODO
			matches: s
				.matches
				.iter()
				.map(RouteMatch::try_from)
				.collect::<Result<Vec<_>, _>>()?,
			filters: s
				.filters
				.iter()
				.map(RouteFilter::try_from)
				.collect::<Result<Vec<_>, _>>()?,
			backends: s
				.backends
				.iter()
				.map(RouteBackend::try_from)
				.collect::<Result<Vec<_>, _>>()?,
			policies: s.traffic_policy.map(TrafficPolicy::try_from).transpose()?,
		};
		Ok((r, strng::new(&s.listener)))
	}
}

impl TryFrom<&proto::adp::RouteMatch> for RouteMatch {
	type Error = ProtoError;

	fn try_from(s: &proto::adp::RouteMatch) -> Result<Self, Self::Error> {
		use proto::adp::path_match::*;
		let path = match &s.path {
			None => PathMatch::PathPrefix(strng::new("/")),
			Some(proto::adp::PathMatch {
				kind: Some(Kind::PathPrefix(prefix)),
			}) => PathMatch::PathPrefix(strng::new(prefix)),
			Some(proto::adp::PathMatch {
				kind: Some(Kind::Exact(prefix)),
			}) => PathMatch::Exact(strng::new(prefix)),
			Some(proto::adp::PathMatch {
				kind: Some(Kind::Regex(r)),
			}) => PathMatch::Regex(regex::Regex::new(r)?, r.len()),
			Some(proto::adp::PathMatch { kind: None }) => {
				return Err(ProtoError::Generic("invalid path match".to_string()));
			},
		};
		let method = s.method.as_ref().map(|m| MethodMatch {
			method: strng::new(&m.exact),
		});
		let headers = s
			.headers
			.iter()
			.map(|h| match &h.value {
				None => Err(ProtoError::Generic(
					"invalid header match value".to_string(),
				)),
				Some(proto::adp::header_match::Value::Exact(e)) => Ok(HeaderMatch {
					name: http::HeaderName::from_bytes(h.name.as_bytes())?,
					value: HeaderValueMatch::Exact(http::HeaderValue::from_bytes(e.as_bytes())?),
				}),
				Some(proto::adp::header_match::Value::Regex(e)) => Ok(HeaderMatch {
					name: http::HeaderName::from_bytes(h.name.as_bytes())?,
					value: HeaderValueMatch::Regex(regex::Regex::new(e)?),
				}),
			})
			.collect::<Result<Vec<_>, _>>()?;
		let query = s
			.query_params
			.iter()
			.map(|h| match &h.value {
				None => Err(ProtoError::Generic("invalid query match value".to_string())),
				Some(proto::adp::query_match::Value::Exact(e)) => Ok(QueryMatch {
					name: strng::new(&h.name),
					value: QueryValueMatch::Exact(strng::new(e)),
				}),
				Some(proto::adp::query_match::Value::Regex(e)) => Ok(QueryMatch {
					name: strng::new(&h.name),
					value: QueryValueMatch::Regex(regex::Regex::new(e)?),
				}),
			})
			.collect::<Result<Vec<_>, _>>()?;
		Ok(Self {
			headers,
			path,
			method,
			query,
		})
	}
}

impl TryFrom<&proto::adp::RouteFilter> for RouteFilter {
	type Error = ProtoError;

	fn try_from(s: &proto::adp::RouteFilter) -> Result<Self, Self::Error> {
		Ok(match &s.kind {
			None => return Err(ProtoError::Generic("invalid route filter".to_string())),
			Some(proto::adp::route_filter::Kind::RequestHeaderModifier(rhm)) => {
				RouteFilter::RequestHeaderModifier(filters::HeaderModifier {
					add: rhm
						.add
						.iter()
						.map(|h| (strng::new(&h.name), strng::new(&h.value)))
						.collect(),
					set: rhm
						.set
						.iter()
						.map(|h| (strng::new(&h.name), strng::new(&h.value)))
						.collect(),
					remove: rhm.remove.iter().map(strng::new).collect(),
				})
			},
			Some(proto::adp::route_filter::Kind::RequestRedirect(rd)) => {
				RouteFilter::RequestRedirect(filters::RequestRedirect {
					scheme: default_as_none(rd.scheme.as_str())
						.map(uri::Scheme::try_from)
						.transpose()?,
					authority: match (default_as_none(rd.host.as_str()), default_as_none(rd.port)) {
						(Some(h), Some(p)) => Some(HostRedirect::Full(strng::format!("{h}:{p}"))),
						(_, Some(p)) => Some(HostRedirect::Port(NonZeroU16::new(p as u16).unwrap())),
						(Some(h), _) => Some(HostRedirect::Host(strng::new(h))),
						(None, None) => None,
					},
					path: match &rd.path {
						Some(proto::adp::request_redirect::Path::Full(f)) => {
							Some(PathRedirect::Full(strng::new(f)))
						},
						Some(proto::adp::request_redirect::Path::Prefix(f)) => {
							Some(PathRedirect::Prefix(strng::new(f)))
						},
						None => None,
					},
					status: default_as_none(rd.status)
						.map(|i| StatusCode::from_u16(i as u16))
						.transpose()?,
				})
			},
			Some(proto::adp::route_filter::Kind::UrlRewrite(rw)) => {
				RouteFilter::UrlRewrite(filters::UrlRewrite {
					authority: default_as_none(rw.host.as_str()).map(|h| HostRedirect::Host(strng::new(h))),
					path: match &rw.path {
						Some(proto::adp::url_rewrite::Path::Full(f)) => Some(PathRedirect::Full(strng::new(f))),
						Some(proto::adp::url_rewrite::Path::Prefix(f)) => {
							Some(PathRedirect::Prefix(strng::new(f)))
						},
						None => None,
					},
				})
			},
			Some(proto::adp::route_filter::Kind::ResponseHeaderModifier(rhm)) => {
				RouteFilter::ResponseHeaderModifier(filters::HeaderModifier {
					add: rhm
						.add
						.iter()
						.map(|h| (strng::new(&h.name), strng::new(&h.value)))
						.collect(),
					set: rhm
						.set
						.iter()
						.map(|h| (strng::new(&h.name), strng::new(&h.value)))
						.collect(),
					remove: rhm.remove.iter().map(strng::new).collect(),
				})
			},
			Some(proto::adp::route_filter::Kind::RequestMirror(m)) => {
				RouteFilter::RequestMirror(filters::RequestMirror {
					backend: match &m.kind {
						None => Backend::Invalid,
						Some(proto::adp::request_mirror::Kind::Service(svc_key)) => {
							let ns = match svc_key.split_once('/') {
								Some((namespace, hostname)) => Ok(NamespacedHostname {
									namespace: namespace.into(),
									hostname: hostname.into(),
								}),
								None => Err(ProtoError::NamespacedHostnameParse(svc_key.clone())),
							}?;
							Backend::Service(ns)
						},
					},
					port: m.port as u16,
					percentage: m.percentage / 100.0,
				})
			},
		})
	}
}
