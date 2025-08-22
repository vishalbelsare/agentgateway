use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU16;
use std::sync::Arc;

use rustls::ServerConfig;

use super::agent::*;
use crate::http::auth::{AwsAuth, BackendAuth, SimpleBackendAuth};
use crate::http::{StatusCode, authorization, backendtls, ext_proc, filters, localratelimit, uri};
use crate::llm::{AIBackend, AIProvider};
use crate::mcp::rbac::McpAuthorization;
use crate::types::discovery::NamespacedHostname;
use crate::types::proto;
use crate::types::proto::ProtoError;
use crate::types::proto::agent::mcp_target::Protocol;
use crate::types::proto::agent::policy_spec::inference_routing::FailureMode;
use crate::types::proto::agent::policy_spec::local_rate_limit::Type;
use crate::*;

impl TryFrom<&proto::agent::TlsConfig> for TLSConfig {
	type Error = anyhow::Error;

	fn try_from(value: &proto::agent::TlsConfig) -> Result<Self, Self::Error> {
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

impl TryFrom<&proto::agent::RouteBackend> for RouteBackendReference {
	type Error = ProtoError;

	fn try_from(s: &proto::agent::RouteBackend) -> Result<Self, Self::Error> {
		let kind = resolve_reference(s.backend.as_ref())?;
		let filters = s
			.filters
			.iter()
			.map(RouteFilter::try_from)
			.collect::<Result<Vec<_>, _>>()?;
		Ok(Self {
			weight: s.weight as usize,
			backend: kind,
			filters,
		})
	}
}

impl TryFrom<proto::agent::BackendAuthPolicy> for BackendAuth {
	type Error = ProtoError;

	fn try_from(s: proto::agent::BackendAuthPolicy) -> Result<Self, Self::Error> {
		Ok(match s.kind {
			Some(proto::agent::backend_auth_policy::Kind::Passthrough(_)) => BackendAuth::Passthrough {},
			Some(proto::agent::backend_auth_policy::Kind::Key(k)) => BackendAuth::Key(k.secret.into()),
			Some(proto::agent::backend_auth_policy::Kind::Gcp(_)) => BackendAuth::Gcp {},
			Some(proto::agent::backend_auth_policy::Kind::Aws(a)) => {
				let aws_auth = match a.kind {
					Some(proto::agent::aws::Kind::ExplicitConfig(config)) => AwsAuth::ExplicitConfig {
						access_key_id: config.access_key_id.into(),
						secret_access_key: config.secret_access_key.into(),
						region: config.region,
						session_token: config.session_token.map(|token| token.into()),
					},
					Some(proto::agent::aws::Kind::Implicit(_)) => AwsAuth::Implicit {},
					None => return Err(ProtoError::MissingRequiredField),
				};
				BackendAuth::Aws(aws_auth)
			},
			None => return Err(ProtoError::MissingRequiredField),
		})
	}
}

impl TryFrom<proto::agent::TrafficPolicy> for TrafficPolicy {
	type Error = ProtoError;

	fn try_from(s: proto::agent::TrafficPolicy) -> Result<Self, Self::Error> {
		let req = s.request_timeout.map(|v| v.try_into()).transpose()?;
		let backend = s
			.backend_request_timeout
			.map(|v| v.try_into())
			.transpose()?;

		let retry = s
			.retry
			.map(
				|retry_proto| -> Result<crate::http::retry::Policy, ProtoError> {
					let codes: Result<Vec<http::StatusCode>, _> = retry_proto
						.retry_status_codes
						.iter()
						.map(|&v| {
							http::StatusCode::from_u16(v as u16)
								.map_err(|_| ProtoError::Generic(format!("invalid status code: {v}")))
						})
						.collect();
					Ok(crate::http::retry::Policy {
						codes: codes?.into_boxed_slice(),
						attempts: std::num::NonZeroU8::new(retry_proto.attempts as u8)
							.unwrap_or_else(|| std::num::NonZeroU8::new(1).unwrap()),
						backoff: retry_proto.backoff.map(|v| v.try_into()).transpose()?,
					})
				},
			)
			.transpose()?;

		Ok(Self {
			timeout: crate::http::timeout::Policy {
				request_timeout: req,
				backend_request_timeout: backend,
			},
			retry,
		})
	}
}

impl TryFrom<(proto::agent::Protocol, Option<&proto::agent::TlsConfig>)> for ListenerProtocol {
	type Error = ProtoError;
	fn try_from(
		value: (proto::agent::Protocol, Option<&proto::agent::TlsConfig>),
	) -> Result<Self, Self::Error> {
		use crate::types::proto::agent::Protocol;
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

impl TryFrom<&proto::agent::Bind> for Bind {
	type Error = ProtoError;

	fn try_from(s: &proto::agent::Bind) -> Result<Self, Self::Error> {
		Ok(Self {
			key: s.key.clone().into(),
			address: SocketAddr::from((IpAddr::from([0, 0, 0, 0]), s.port as u16)),
			listeners: Default::default(),
		})
	}
}

impl TryFrom<&proto::agent::Listener> for (Listener, BindName) {
	type Error = ProtoError;

	fn try_from(s: &proto::agent::Listener) -> Result<Self, Self::Error> {
		let proto = proto::agent::Protocol::try_from(s.protocol)?;
		let protocol = ListenerProtocol::try_from((proto, s.tls.as_ref()))
			.map_err(|e| ProtoError::Generic(format!("{e}")))?;
		let l = Listener {
			key: strng::new(&s.key),
			name: strng::new(&s.name),
			hostname: s.hostname.clone().into(),
			protocol,
			gateway_name: strng::new(&s.gateway_name),
			routes: Default::default(),
			tcp_routes: Default::default(),
		};
		Ok((l, strng::new(&s.bind_key)))
	}
}

impl TryFrom<&proto::agent::TcpRoute> for (TCPRoute, ListenerKey) {
	type Error = ProtoError;

	fn try_from(s: &proto::agent::TcpRoute) -> Result<Self, Self::Error> {
		let r = TCPRoute {
			key: strng::new(&s.key),
			route_name: strng::new(&s.route_name),
			rule_name: default_as_none(s.rule_name.as_str()).map(strng::new),
			hostnames: s.hostnames.iter().map(strng::new).collect(),
			backends: s
				.backends
				.iter()
				.map(|b| -> Result<TCPRouteBackendReference, ProtoError> {
					Ok(TCPRouteBackendReference {
						weight: b.weight as usize,
						backend: resolve_simple_reference(b.backend.as_ref())?,
					})
				})
				.collect::<Result<Vec<_>, _>>()?,
		};
		Ok((r, strng::new(&s.listener_key)))
	}
}

impl TryFrom<&proto::agent::Route> for (Route, ListenerKey) {
	type Error = ProtoError;

	fn try_from(s: &proto::agent::Route) -> Result<Self, Self::Error> {
		let r = Route {
			key: strng::new(&s.key),
			route_name: strng::new(&s.route_name),
			rule_name: default_as_none(s.rule_name.as_str()).map(strng::new),
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
				.map(RouteBackendReference::try_from)
				.collect::<Result<Vec<_>, _>>()?,
			policies: s
				.traffic_policy
				.clone()
				.map(TrafficPolicy::try_from)
				.transpose()?,
			inline_policies: s
				.inline_policies
				.iter()
				.map(Policy::try_from)
				.collect::<Result<Vec<_>, _>>()?,
		};
		Ok((r, strng::new(&s.listener_key)))
	}
}

impl TryFrom<&proto::agent::Backend> for Backend {
	type Error = ProtoError;

	fn try_from(s: &proto::agent::Backend) -> Result<Self, Self::Error> {
		let name = BackendName::from(&s.name);
		let backend = match &s.kind {
			Some(proto::agent::backend::Kind::Static(s)) => Backend::Opaque(
				name.clone(),
				Target::try_from((s.host.as_str(), s.port as u16))
					.map_err(|e| ProtoError::Generic(e.to_string()))?,
			),
			Some(proto::agent::backend::Kind::Ai(a)) => Backend::AI(
				name.clone(),
				AIBackend {
					tokenize: false,
					host_override: a
						.r#override
						.as_ref()
						.map(|o| {
							Target::try_from((o.host.as_str(), o.port as u16))
								.map_err(|e| ProtoError::Generic(e.to_string()))
						})
						.transpose()?,
					provider: match &a.provider {
						Some(proto::agent::ai_backend::Provider::Openai(openai)) => {
							AIProvider::OpenAI(llm::openai::Provider {
								model: openai.model.as_deref().map(strng::new),
							})
						},
						Some(proto::agent::ai_backend::Provider::Gemini(gemini)) => {
							AIProvider::Gemini(llm::gemini::Provider {
								model: gemini.model.as_deref().map(strng::new),
							})
						},
						Some(proto::agent::ai_backend::Provider::Vertex(vertex)) => {
							AIProvider::Vertex(llm::vertex::Provider {
								model: vertex.model.as_deref().map(strng::new),
								region: Some(strng::new(&vertex.region)),
								project_id: strng::new(&vertex.project_id),
							})
						},
						Some(proto::agent::ai_backend::Provider::Anthropic(anthropic)) => {
							AIProvider::Anthropic(llm::anthropic::Provider {
								model: anthropic.model.as_deref().map(strng::new),
							})
						},
						Some(proto::agent::ai_backend::Provider::Bedrock(bedrock)) => {
							AIProvider::Bedrock(llm::bedrock::Provider {
								model: bedrock.model.as_deref().map(strng::new),
								region: strng::new(&bedrock.region),
								guardrail_identifier: bedrock.guardrail_identifier.as_deref().map(strng::new),
								guardrail_version: bedrock.guardrail_version.as_deref().map(strng::new),
							})
						},
						None => {
							return Err(ProtoError::Generic(
								"AI backend provider is required".to_string(),
							));
						},
					},
				},
			),
			Some(proto::agent::backend::Kind::Mcp(m)) => Backend::MCP(
				name.clone(),
				McpBackend {
					targets: m
						.targets
						.iter()
						.map(|t| McpTarget::try_from(t).map(Arc::new))
						.collect::<Result<Vec<_>, _>>()?,
					stateful: match m.stateful_mode() {
						proto::agent::mcp_backend::StatefulMode::Stateful => true,
						proto::agent::mcp_backend::StatefulMode::Stateless => false,
					},
				},
			),
			_ => {
				return Err(ProtoError::Generic("unknown backend".to_string()));
			},
		};
		Ok(backend)
	}
}

impl TryFrom<&proto::agent::McpTarget> for McpTarget {
	type Error = ProtoError;

	fn try_from(s: &proto::agent::McpTarget) -> Result<Self, Self::Error> {
		let proto = proto::agent::mcp_target::Protocol::try_from(s.protocol)?;
		let backend = resolve_simple_reference(s.backend.as_ref())?;

		Ok(Self {
			name: strng::new(&s.name),
			spec: match proto {
				Protocol::Sse => McpTargetSpec::Sse(SseTargetSpec {
					backend,
					path: if s.path.is_empty() {
						"/sse".to_string()
					} else {
						s.path.clone()
					},
				}),
				Protocol::Undefined | Protocol::StreamableHttp => {
					McpTargetSpec::Mcp(StreamableHTTPTargetSpec {
						backend,
						path: if s.path.is_empty() {
							"/mcp".to_string()
						} else {
							s.path.clone()
						},
					})
				},
			},
		})
	}
}

impl TryFrom<&proto::agent::RouteMatch> for RouteMatch {
	type Error = ProtoError;

	fn try_from(s: &proto::agent::RouteMatch) -> Result<Self, Self::Error> {
		use crate::types::proto::agent::path_match::*;
		let path = match &s.path {
			None => PathMatch::PathPrefix(strng::new("/")),
			Some(proto::agent::PathMatch {
				kind: Some(Kind::PathPrefix(prefix)),
			}) => PathMatch::PathPrefix(strng::new(prefix)),
			Some(proto::agent::PathMatch {
				kind: Some(Kind::Exact(prefix)),
			}) => PathMatch::Exact(strng::new(prefix)),
			Some(proto::agent::PathMatch {
				kind: Some(Kind::Regex(r)),
			}) => PathMatch::Regex(regex::Regex::new(r)?, r.len()),
			Some(proto::agent::PathMatch { kind: None }) => {
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
				Some(proto::agent::header_match::Value::Exact(e)) => Ok(HeaderMatch {
					name: crate::http::HeaderName::from_bytes(h.name.as_bytes())?,
					value: HeaderValueMatch::Exact(crate::http::HeaderValue::from_bytes(e.as_bytes())?),
				}),
				Some(proto::agent::header_match::Value::Regex(e)) => Ok(HeaderMatch {
					name: crate::http::HeaderName::from_bytes(h.name.as_bytes())?,
					value: HeaderValueMatch::Regex(regex::Regex::new(e)?),
				}),
			})
			.collect::<Result<Vec<_>, _>>()?;
		let query = s
			.query_params
			.iter()
			.map(|h| match &h.value {
				None => Err(ProtoError::Generic("invalid query match value".to_string())),
				Some(proto::agent::query_match::Value::Exact(e)) => Ok(QueryMatch {
					name: strng::new(&h.name),
					value: QueryValueMatch::Exact(strng::new(e)),
				}),
				Some(proto::agent::query_match::Value::Regex(e)) => Ok(QueryMatch {
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

impl TryFrom<&proto::agent::RouteFilter> for RouteFilter {
	type Error = ProtoError;

	fn try_from(s: &proto::agent::RouteFilter) -> Result<Self, Self::Error> {
		Ok(match &s.kind {
			None => return Err(ProtoError::Generic("invalid route filter".to_string())),
			Some(proto::agent::route_filter::Kind::RequestHeaderModifier(rhm)) => {
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
			Some(proto::agent::route_filter::Kind::RequestRedirect(rd)) => {
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
						Some(proto::agent::request_redirect::Path::Full(f)) => {
							Some(PathRedirect::Full(strng::new(f)))
						},
						Some(proto::agent::request_redirect::Path::Prefix(f)) => {
							Some(PathRedirect::Prefix(strng::new(f)))
						},
						None => None,
					},
					status: default_as_none(rd.status)
						.map(|i| StatusCode::from_u16(i as u16))
						.transpose()?,
				})
			},
			Some(proto::agent::route_filter::Kind::UrlRewrite(rw)) => {
				RouteFilter::UrlRewrite(filters::UrlRewrite {
					authority: default_as_none(rw.host.as_str()).map(|h| HostRedirect::Host(strng::new(h))),
					path: match &rw.path {
						Some(proto::agent::url_rewrite::Path::Full(f)) => {
							Some(PathRedirect::Full(strng::new(f)))
						},
						Some(proto::agent::url_rewrite::Path::Prefix(f)) => {
							Some(PathRedirect::Prefix(strng::new(f)))
						},
						None => None,
					},
				})
			},
			Some(proto::agent::route_filter::Kind::ResponseHeaderModifier(rhm)) => {
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
			Some(proto::agent::route_filter::Kind::RequestMirror(m)) => {
				let backend = resolve_simple_reference(m.backend.as_ref())?;
				RouteFilter::RequestMirror(filters::RequestMirror {
					backend,
					percentage: m.percentage / 100.0,
				})
			},
			Some(proto::agent::route_filter::Kind::DirectResponse(m)) => {
				RouteFilter::DirectResponse(filters::DirectResponse {
					body: Bytes::copy_from_slice(&m.body),
					status: StatusCode::from_u16(m.status as u16)?,
				})
			},
			Some(proto::agent::route_filter::Kind::Cors(c)) => RouteFilter::CORS(
				http::cors::Cors::try_from(http::cors::CorsSerde {
					allow_credentials: c.allow_credentials,
					allow_headers: c.allow_headers.clone(),
					allow_methods: c.allow_methods.clone(),
					allow_origins: c.allow_origins.clone(),
					expose_headers: c.expose_headers.clone(),
					max_age: c.max_age.map(|d| Duration::from_secs(d.seconds as u64)),
				})
				.map_err(|e| ProtoError::Generic(e.to_string()))?,
			),
		})
	}
}

fn default_as_none<T: Default + PartialEq>(i: T) -> Option<T> {
	if i == Default::default() {
		None
	} else {
		Some(i)
	}
}

impl TryFrom<&proto::agent::policy_spec::Rbac> for Authorization {
	type Error = ProtoError;

	fn try_from(rbac: &proto::agent::policy_spec::Rbac) -> Result<Self, Self::Error> {
		// Convert allow rules
		let mut allow_exprs = Vec::new();
		for allow_rule in &rbac.allow {
			let expr = cel::Expression::new(allow_rule)
				.map_err(|e| ProtoError::Generic(format!("invalid CEL expression in allow rule: {e}")))?;
			allow_exprs.push(Arc::new(expr));
		}
		// Convert deny rules
		let mut deny_exprs = Vec::new();
		for deny_rule in &rbac.deny {
			let expr = cel::Expression::new(deny_rule)
				.map_err(|e| ProtoError::Generic(format!("invalid CEL expression in deny rule: {e}")))?;
			deny_exprs.push(Arc::new(expr));
		}

		// Create PolicySet using the same pattern as in de_policies function
		let policy_set = authorization::PolicySet::new(allow_exprs, deny_exprs);
		Ok(Authorization(authorization::RuleSet::new(policy_set)))
	}
}

impl TryFrom<&proto::agent::policy_spec::Rbac> for McpAuthorization {
	type Error = ProtoError;

	fn try_from(rbac: &proto::agent::policy_spec::Rbac) -> Result<Self, Self::Error> {
		// Convert allow rules
		let mut allow_exprs = Vec::new();
		for allow_rule in &rbac.allow {
			let expr = cel::Expression::new(allow_rule)
				.map_err(|e| ProtoError::Generic(format!("invalid CEL expression in allow rule: {e}")))?;
			allow_exprs.push(Arc::new(expr));
		}

		// Convert deny rules
		let mut deny_exprs = Vec::new();
		for deny_rule in &rbac.deny {
			let expr = cel::Expression::new(deny_rule)
				.map_err(|e| ProtoError::Generic(format!("invalid CEL expression in deny rule: {e}")))?;
			deny_exprs.push(Arc::new(expr));
		}

		// Create PolicySet using the same pattern as in de_policies function
		let policy_set = authorization::PolicySet::new(allow_exprs, deny_exprs);
		Ok(McpAuthorization::new(authorization::RuleSet::new(
			policy_set,
		)))
	}
}

impl TryFrom<&proto::agent::PolicySpec> for Policy {
	type Error = ProtoError;
	fn try_from(spec: &proto::agent::PolicySpec) -> Result<Self, Self::Error> {
		Ok(match &spec.kind {
			Some(proto::agent::policy_spec::Kind::LocalRateLimit(lrl)) => {
				let t = proto::agent::policy_spec::local_rate_limit::Type::try_from(lrl.r#type)?;
				Policy::LocalRateLimit(vec![
					localratelimit::RateLimitSerde {
						max_tokens: lrl.max_tokens,
						tokens_per_fill: lrl.tokens_per_fill,
						fill_interval: lrl
							.fill_interval
							.ok_or(ProtoError::MissingRequiredField)?
							.try_into()?,
						limit_type: match t {
							Type::Request => localratelimit::RateLimitType::Requests,
							Type::Token => localratelimit::RateLimitType::Tokens,
						},
					}
					.try_into()
					.map_err(|e| ProtoError::Generic(format!("invalid rate limit: {e}")))?,
				])
			},
			Some(proto::agent::policy_spec::Kind::ExtAuthz(ea)) => {
				let target = resolve_simple_reference(ea.target.as_ref())?;
				Policy::ExtAuthz(http::ext_authz::ExtAuthz {
					target: Arc::new(target),
					context: Some(ea.context.clone()),
				})
			},
			Some(proto::agent::policy_spec::Kind::A2a(_)) => Policy::A2a(A2aPolicy {}),
			Some(proto::agent::policy_spec::Kind::BackendTls(btls)) => {
				let tls = backendtls::ResolvedBackendTLS {
					cert: btls.cert.clone(),
					key: btls.key.clone(),
					root: btls.root.clone(),
					insecure: btls.insecure.unwrap_or_default(),
					insecure_host: false,
					hostname: btls.hostname.clone(),
				}
				.try_into()
				.map_err(|e| ProtoError::Generic(e.to_string()))?;
				Policy::BackendTLS(tls)
			},
			Some(proto::agent::policy_spec::Kind::InferenceRouting(ir)) => {
				Policy::InferenceRouting(ext_proc::InferenceRouting {
					target: Arc::new(resolve_simple_reference(ir.endpoint_picker.as_ref())?),
					failure_mode: match proto::agent::policy_spec::inference_routing::FailureMode::try_from(
						ir.failure_mode,
					)? {
						FailureMode::Unknown | FailureMode::FailClosed => ext_proc::FailureMode::FailClosed,
						FailureMode::FailOpen => ext_proc::FailureMode::FailOpen,
					},
				})
			},
			Some(proto::agent::policy_spec::Kind::Auth(auth)) => {
				Policy::BackendAuth(BackendAuth::try_from(auth.clone())?)
			},
			Some(proto::agent::policy_spec::Kind::Authorization(rbac)) => {
				Policy::Authorization(Authorization::try_from(rbac)?)
			},
			Some(proto::agent::policy_spec::Kind::McpAuthorization(rbac)) => {
				Policy::McpAuthorization(McpAuthorization::try_from(rbac)?)
			},
			Some(proto::agent::policy_spec::Kind::Ai(ai)) => {
				let prompt_guard = ai.prompt_guard.as_ref().and_then(|pg| {
					let reqp = pg.request.as_ref()?;

					let rejection = if let Some(resp) = &reqp.rejection {
						let status = u16::try_from(resp.status)
							.ok()
							.and_then(|c| StatusCode::from_u16(c).ok())
							.unwrap_or(StatusCode::FORBIDDEN);
						crate::llm::policy::RequestRejection {
							body: Bytes::from(resp.body.clone()),
							status,
						}
					} else {
						//  use default response, since the response field is not optional on RequestGuard
						crate::llm::policy::RequestRejection::default()
					};

					let regex = reqp
						.regex
						.as_ref()
						.map(|rr| convert_regex_rules(rr, Some(rejection.clone())));

					let webhook = reqp.webhook.as_ref().and_then(convert_webhook);

					let openai_moderation =
						reqp
							.openai_moderation
							.as_ref()
							.map(|m| crate::llm::policy::Moderation {
								model: m.model.as_deref().map(strng::new),
								auth: match m.auth.as_ref().and_then(|a| a.kind.clone()) {
									Some(crate::types::proto::agent::backend_auth_policy::Kind::Passthrough(_)) => {
										SimpleBackendAuth::Passthrough {}
									},
									Some(crate::types::proto::agent::backend_auth_policy::Kind::Key(k)) => {
										SimpleBackendAuth::Key(k.secret.into())
									},
									_ => SimpleBackendAuth::Passthrough {},
								},
							});

					Some(crate::llm::policy::PromptGuard {
						request: Some(crate::llm::policy::RequestGuard {
							rejection,
							regex,
							webhook,
							openai_moderation,
						}),
						response: pg
							.response
							.as_ref()
							.map(|resp| crate::llm::policy::ResponseGuard {
								regex: resp.regex.as_ref().map(|rr| convert_regex_rules(rr, None)),
								webhook: resp.webhook.as_ref().and_then(convert_webhook),
							}),
					})
				});

				Policy::AI(Arc::new(llm::Policy {
					prompt_guard,
					defaults: Some(
						ai.defaults
							.iter()
							.map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
							.collect(),
					),
					overrides: Some(
						ai.overrides
							.iter()
							.map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
							.collect(),
					),
					prompts: ai.prompts.as_ref().map(convert_prompt_enrichment),
				}))
			},
			_ => return Err(ProtoError::EnumParse("unknown spec kind".to_string())),
		})
	}
}
impl TryFrom<&proto::agent::Policy> for TargetedPolicy {
	type Error = ProtoError;

	fn try_from(s: &proto::agent::Policy) -> Result<Self, Self::Error> {
		let name = PolicyName::from(&s.name);
		let target = s.target.as_ref().ok_or(ProtoError::MissingRequiredField)?;
		let spec = s.spec.as_ref().ok_or(ProtoError::MissingRequiredField)?;
		let target = match &target.kind {
			Some(proto::agent::policy_target::Kind::Gateway(v)) => PolicyTarget::Gateway(v.into()),
			Some(proto::agent::policy_target::Kind::Listener(v)) => PolicyTarget::Listener(v.into()),
			Some(proto::agent::policy_target::Kind::Route(v)) => PolicyTarget::Route(v.into()),
			Some(proto::agent::policy_target::Kind::RouteRule(v)) => PolicyTarget::RouteRule(v.into()),
			Some(proto::agent::policy_target::Kind::Service(v)) => PolicyTarget::Service(v.into()),
			Some(proto::agent::policy_target::Kind::Backend(v)) => PolicyTarget::Backend(v.into()),
			_ => return Err(ProtoError::EnumParse("unknown target kind".to_string())),
		};
		let policy = spec.try_into()?;
		Ok(TargetedPolicy {
			name,
			target,
			policy,
		})
	}
}

fn resolve_simple_reference(
	target: Option<&proto::agent::BackendReference>,
) -> Result<SimpleBackendReference, ProtoError> {
	let Some(target) = target else {
		return Ok(SimpleBackendReference::Invalid);
	};
	Ok(match target.kind.as_ref() {
		None => SimpleBackendReference::Invalid,
		Some(proto::agent::backend_reference::Kind::Service(svc_key)) => {
			let ns = match svc_key.split_once('/') {
				Some((namespace, hostname)) => Ok(NamespacedHostname {
					namespace: namespace.into(),
					hostname: hostname.into(),
				}),
				None => Err(ProtoError::NamespacedHostnameParse(svc_key.clone())),
			}?;
			SimpleBackendReference::Service {
				name: ns,
				port: target.port as u16,
			}
		},
		Some(proto::agent::backend_reference::Kind::Backend(name)) => {
			SimpleBackendReference::Backend(name.into())
		},
	})
}

fn convert_message(
	m: &proto::agent::policy_spec::ai::Message,
) -> crate::llm::universal::RequestMessage {
	match m.role.as_str() {
		"system" => crate::llm::universal::RequestSystemMessage::from(m.content.clone()).into(),
		"assistant" => crate::llm::universal::RequestAssistantMessage::from(m.content.clone()).into(),
		"function" => crate::llm::universal::RequestFunctionMessage {
			content: Some(m.content.clone()),
			name: "".to_string(),
		}
		.into(),
		"tool" => crate::llm::universal::RequestToolMessage {
			content: crate::llm::universal::RequestToolMessageContent::from(m.content.clone()),
			tool_call_id: "".to_string(),
		}
		.into(),
		_ => crate::llm::universal::RequestUserMessage::from(m.content.clone()).into(),
	}
}

fn convert_prompt_enrichment(
	prompts: &proto::agent::policy_spec::ai::PromptEnrichment,
) -> crate::llm::policy::PromptEnrichment {
	crate::llm::policy::PromptEnrichment {
		append: prompts.append.iter().map(convert_message).collect(),
		prepend: prompts.prepend.iter().map(convert_message).collect(),
	}
}

fn convert_webhook(
	w: &proto::agent::policy_spec::ai::Webhook,
) -> Option<crate::llm::policy::Webhook> {
	match u16::try_from(w.port) {
		Ok(port) => Some(crate::llm::policy::Webhook {
			target: Target::Hostname(w.host.clone().into(), port),
		}),
		Err(_) => {
			warn!(port = w.port, host = %w.host, "Webhook port out of range, ignoring webhook");
			None
		},
	}
}

fn convert_regex_rules(
	rr: &proto::agent::policy_spec::ai::RegexRules,
	rejection: Option<crate::llm::policy::RequestRejection>,
) -> crate::llm::policy::RegexRules {
	let action = match rr
		.action
		.as_ref()
		.and_then(|a| proto::agent::policy_spec::ai::ActionKind::try_from(a.kind).ok())
	{
		Some(proto::agent::policy_spec::ai::ActionKind::Reject) => crate::llm::policy::Action::Reject {
			response: rejection.unwrap_or_default(),
		},
		_ => crate::llm::policy::Action::Mask,
	};
	let rules = rr
		.rules
		.iter()
		.filter_map(|r| match &r.kind {
			Some(proto::agent::policy_spec::ai::regex_rule::Kind::Builtin(b)) => {
				match proto::agent::policy_spec::ai::BuiltinRegexRule::try_from(*b) {
					Ok(builtin) => {
						let builtin = match builtin {
							proto::agent::policy_spec::ai::BuiltinRegexRule::Ssn => {
								crate::llm::policy::Builtin::Ssn
							},
							proto::agent::policy_spec::ai::BuiltinRegexRule::CreditCard => {
								crate::llm::policy::Builtin::CreditCard
							},
							proto::agent::policy_spec::ai::BuiltinRegexRule::PhoneNumber => {
								crate::llm::policy::Builtin::PhoneNumber
							},
							proto::agent::policy_spec::ai::BuiltinRegexRule::Email => {
								crate::llm::policy::Builtin::Email
							},
							_ => {
								warn!(value = *b, "Unknown builtin regex rule, skipping");
								return None;
							},
						};
						Some(crate::llm::policy::RegexRule::Builtin { builtin })
					},
					Err(_) => {
						warn!(value = *b, "Invalid builtin regex rule value, skipping");
						None
					},
				}
			},
			Some(proto::agent::policy_spec::ai::regex_rule::Kind::Regex(n)) => {
				match regex::Regex::new(&n.pattern) {
					Ok(pattern) => Some(crate::llm::policy::RegexRule::Regex {
						pattern,
						name: n.name.clone(),
					}),
					Err(err) => {
						warn!(error = %err, name = %n.name, pattern = %n.pattern, "Invalid regex pattern");
						None
					},
				}
			},
			None => None,
		})
		.collect();
	crate::llm::policy::RegexRules { action, rules }
}

fn resolve_reference(
	target: Option<&proto::agent::BackendReference>,
) -> Result<BackendReference, ProtoError> {
	let Some(target) = target else {
		return Ok(BackendReference::Invalid);
	};
	Ok(match target.kind.as_ref() {
		None => BackendReference::Invalid,
		Some(proto::agent::backend_reference::Kind::Service(svc_key)) => {
			let ns = match svc_key.split_once('/') {
				Some((namespace, hostname)) => Ok(NamespacedHostname {
					namespace: namespace.into(),
					hostname: hostname.into(),
				}),
				None => Err(ProtoError::NamespacedHostnameParse(svc_key.clone())),
			}?;
			BackendReference::Service {
				name: ns,
				port: target.port as u16,
			}
		},
		Some(proto::agent::backend_reference::Kind::Backend(name)) => {
			BackendReference::Backend(name.into())
		},
	})
}
