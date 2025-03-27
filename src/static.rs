use std::sync::Arc;
use tracing::{debug, info, trace};

use rmcp::serve_server;

use crate::proxyprotocol;
use crate::rbac;
use crate::relay;
use crate::relay::Relay;
use crate::sse::App as SseApp;
use crate::xds::{Listener, ListenerMode, Target, XdsStore as ProxyState};

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StaticConfig {
	#[serde(default)]
	pub targets: Vec<Target>,
	#[serde(default)]
	pub policies: Vec<rbac::Rule>,
	#[serde(default)]
	pub listener: Listener,
}

pub async fn run_local_client(
	cfg: &StaticConfig,
	state_ref: Arc<std::sync::RwLock<ProxyState>>,
	metrics: Arc<relay::metrics::Metrics>,
) -> Result<(), ServingError> {
	debug!(
		"load local config: {}",
		serde_yaml::to_string(&cfg).unwrap_or_default()
	);
	// Clear the state
	let state_clone = state_ref.clone();
	{
		let mut state = state_clone.write().unwrap();
		state.targets.clear();
		state.policies.clear();
		let num_targets = cfg.targets.len();
		let num_policies = cfg.policies.len();
		for target in cfg.targets.clone() {
			trace!("inserting target {}", &target.name);
			state.targets.insert(target);
		}
		let rule_set = rbac::RuleSet::new("test".to_string(), "test".to_string(), cfg.policies.clone());
		state.policies.insert(rule_set);
		info!(%num_targets, %num_policies, "local config initialized");
	}

	serve_static_listener(cfg.listener.clone(), state_ref, metrics).await
}

#[derive(Debug)]
pub enum ServingError {
	Sse(std::io::Error),
	StdIo(tokio::task::JoinError),
}

pub async fn serve_static_listener(
	listener: Listener,
	state: Arc<std::sync::RwLock<ProxyState>>,
	metrics: Arc<relay::metrics::Metrics>,
) -> std::result::Result<(), ServingError> {
	match listener {
		Listener::Stdio {} => {
			let relay = serve_server(
				// TODO: This is a hack
				Relay::new(state.clone(), rbac::Identity::empty(), metrics),
				(tokio::io::stdin(), tokio::io::stdout()),
			)
			.await
			.inspect_err(|e| {
				tracing::error!("serving error: {:?}", e);
			})
			.unwrap();
			tracing::info!("serving stdio");
			relay
				.waiting()
				.await
				.map_err(ServingError::StdIo)
				.map(|_| ())
				.inspect_err(|e| {
					tracing::error!("serving error: {:?}", e);
				})
		},
		Listener::Sse {
			host,
			port,
			mode,
			authn: _,
		} => {
			let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port))
				.await
				.unwrap();
			let app = SseApp::new(state.clone(), metrics);
			let router = app.router();

			let enable_proxy = Some(&ListenerMode::Proxy) == mode.as_ref();

			let listener = proxyprotocol::Listener::new(listener, enable_proxy);
			let svc: axum::extract::connect_info::IntoMakeServiceWithConnectInfo<
				axum::Router,
				proxyprotocol::Address,
			> = router.into_make_service_with_connect_info::<proxyprotocol::Address>();
			info!("serving sse on {}:{}", host, port);
			axum::serve(listener, svc)
				.await
				.map_err(ServingError::Sse)
				.inspect_err(|e| {
					tracing::error!("serving error: {:?}", e);
				})
		},
	}
}
