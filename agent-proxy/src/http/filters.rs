use crate::http::uri::Scheme;
use crate::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use crate::http::{Request, Response};
use crate::types::agent::{Backend, HostRedirect, PathMatch, PathRedirect};
use crate::*;
use anyhow::anyhow;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderModifier {
	#[serde(default, skip_serializing_if = "is_default")]
	pub add: Vec<(Strng, Strng)>,
	#[serde(default, skip_serializing_if = "is_default")]
	pub set: Vec<(Strng, Strng)>,
	#[serde(default, skip_serializing_if = "is_default")]
	pub remove: Vec<Strng>,
}

impl HeaderModifier {
	pub fn apply(&self, headers: &mut HeaderMap<HeaderValue>) {
		for (k, v) in &self.add {
			headers.append(
				HeaderName::from_bytes(k.as_bytes()).unwrap(),
				v.parse().unwrap(),
			);
		}
		for (k, v) in &self.set {
			headers.insert(
				HeaderName::from_bytes(k.as_bytes()).unwrap(),
				v.parse().unwrap(),
			);
		}
		for k in &self.remove {
			headers.remove(HeaderName::from_bytes(k.as_bytes()).unwrap());
		}
	}
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestRedirect {
	#[serde(
		default,
		skip_serializing_if = "is_default",
		serialize_with = "serialize_option_display"
	)]
	pub scheme: Option<http::uri::Scheme>,
	#[serde(default, skip_serializing_if = "is_default")]
	pub authority: Option<HostRedirect>,
	#[serde(default, skip_serializing_if = "is_default")]
	pub path: Option<PathRedirect>,
	#[serde(
		default,
		skip_serializing_if = "is_default",
		serialize_with = "serialize_option_display"
	)]
	pub status: Option<http::StatusCode>,
}

impl RequestRedirect {
	pub fn apply(&self, req: &mut Request, path_match: &PathMatch) -> Option<Response> {
		let RequestRedirect {
			scheme,
			authority,
			path,
			status,
		} = self;
		let new_scheme = scheme
			.as_ref()
			.or_else(|| req.uri().scheme())
			.cloned()
			.unwrap_or(Scheme::HTTP);
		let authority = rewrite_host(authority, req.uri(), scheme.as_ref(), &new_scheme).expect("TODO");
		let path_and_query = rewrite_path(path, path_match, req.uri()).expect("TODO");
		let new = Uri::builder()
			.scheme(new_scheme)
			.authority(authority)
			.path_and_query(path_and_query)
			.build()
			.map_err(|_| anyhow!("invalid redirect"))
			.expect("TODO");
		Some(
			::http::Response::builder()
				.status(status.unwrap_or(StatusCode::FOUND))
				.header(http::header::LOCATION, new.to_string())
				.body(http::Body::empty())
				.map_err(std::io::Error::other)
				.unwrap(),
		)
	}
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UrlRewrite {
	#[serde(default, skip_serializing_if = "is_default")]
	pub authority: Option<HostRedirect>,
	#[serde(default, skip_serializing_if = "is_default")]
	pub path: Option<PathRedirect>,
}

impl UrlRewrite {
	pub fn apply(&self, req: &mut Request, path_match: &PathMatch) {
		let UrlRewrite { authority, path } = self;
		let scheme = req.uri().scheme().cloned().unwrap_or(Scheme::HTTP);

		let new_authority = rewrite_host(authority, req.uri(), Some(&scheme), &scheme).expect("TODO");
		if authority.is_some() {
			req.headers_mut().insert(
				http::header::HOST,
				HeaderValue::from_bytes(new_authority.as_str().as_bytes()).expect("TODO"),
			);
		}
		let path_and_query = rewrite_path(path, path_match, req.uri()).expect("TODO");
		let new = Uri::builder()
			.scheme(scheme)
			.authority(new_authority)
			.path_and_query(path_and_query)
			.build()
			.map_err(|_| anyhow!("invalid redirect"))
			.expect("TODO");
		*req.uri_mut() = new;
	}
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestMirror {
	pub backend: Backend,
	pub port: u16,
	// 0.0-1.0
	pub percentage: f64,
}

fn rewrite_host(
	rewrite: &Option<HostRedirect>,
	orig: &http::Uri,
	original_scheme: Option<&http::uri::Scheme>,
	new_scheme: &http::uri::Scheme,
) -> anyhow::Result<http::uri::Authority> {
	match &rewrite {
		None => orig
			.authority()
			.cloned()
			.ok_or(anyhow::anyhow!("no authority")),
		Some(HostRedirect::Full(hp)) => Ok(hp.as_str().try_into()?),
		Some(HostRedirect::Host(h)) => {
			if original_scheme == Some(&Scheme::HTTP) || original_scheme == Some(&Scheme::HTTPS) {
				Ok(h.as_str().try_into()?)
			} else {
				let new_port = orig
					.port_u16()
					.and_then(|p| port_respecting_default(new_scheme, p));
				match new_port {
					Some(p) => Ok(format!("{}:{}", h, p).try_into()?),
					None => Ok(h.as_str().try_into()?),
				}
			}
		},
		Some(HostRedirect::Port(p)) => {
			match port_respecting_default(new_scheme, p.get()) {
				// We need to set port here
				Some(p) if Some(p) != orig.port_u16() => {
					let h = orig.host().ok_or(anyhow::anyhow!("no authority"))?;
					Ok(format!("{}:{}", h, p).try_into()?)
				},

				// Strip the port
				None if orig.port().is_some() => Ok(
					orig
						.host()
						.ok_or(anyhow::anyhow!("no authority"))?
						.parse()?,
				),

				// Keep it as-is
				_ => orig
					.authority()
					.ok_or(anyhow::anyhow!("no authority"))
					.cloned(),
			}
		},
	}
}

fn port_respecting_default(scheme: &http::uri::Scheme, port: u16) -> Option<u16> {
	if *scheme == http::uri::Scheme::HTTP && port == 80 {
		return None;
	}
	if *scheme == http::uri::Scheme::HTTPS && port == 443 {
		return None;
	}
	Some(port)
}

fn rewrite_path(
	rewrite: &Option<PathRedirect>,
	path_match: &PathMatch,
	orig: &http::Uri,
) -> anyhow::Result<http::uri::PathAndQuery> {
	// TODO: we need to consider the selected match
	match rewrite {
		None => Ok(
			orig
				.path_and_query()
				.ok_or(anyhow!("should have a path"))
				.cloned()?,
		),
		Some(PathRedirect::Full(r)) => Ok(r.as_str().try_into()?),
		Some(PathRedirect::Prefix(r)) => {
			let PathMatch::PathPrefix(match_pfx) = path_match else {
				anyhow::bail!("invalid prefix rewrite")
			};
			let mut new_path = r.to_string();
			let (_, rest) = orig.path().split_at(match_pfx.len());
			if !rest.is_empty() && !rest.starts_with('/') {
				new_path.push('/');
			}
			new_path.push_str(rest);
			if let Some(q) = orig.query() {
				new_path.push('?');
				new_path.push_str(q);
			}
			Ok(new_path.try_into()?)
		},
	}
}
