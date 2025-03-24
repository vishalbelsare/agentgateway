use std::sync::Arc;
use tracing::{debug, info, trace};

use rmcp::{ServerHandlerService, serve_server};

use crate::proxyprotocol;
use crate::rbac;
use crate::relay::Relay;
use crate::sse::App as SseApp;
use crate::state::{Listener, ListenerMode, State as ProxyState, Target};
use axum::http::HeaderMap;
use tokio::sync::RwLock;

#[derive(Default, Debug, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StaticConfig {
	#[serde(default)]
	pub targets: Vec<Target>,
	#[serde(default)]
	pub policies: Vec<rbac::Rule>,
	#[serde(default)]
	pub listener: Listener,
}

pub async fn run_local_client(cfg: StaticConfig) -> Result<(), ServingError> {
	debug!(
		"load local config: {}",
		serde_yaml::to_string(&cfg).unwrap_or_default()
	);
	let mut state = ProxyState::new();
	// Clear the state
	state.targets.clear();
	state.policies.clear();
	let num_targets = cfg.targets.len();
	let num_policies = cfg.policies.len();
	for target in cfg.targets {
		trace!("inserting target {}", &target.name);
		state.targets.insert(target);
	}
	let rule_set = rbac::RuleSet::new("test".to_string(), "test".to_string(), cfg.policies);
	state.policies.insert(rule_set);
	let state = Arc::new(RwLock::new(state));
	let state_clone = state.clone();
	tokio::spawn(async move {
		// This is permanently holding the lock
		let state = state_clone.read().await;
		state.run().await.unwrap();
	});
	info!(%num_targets, %num_policies, "local config initialized");
	serve(cfg.listener, state).await
}

#[derive(Debug)]
pub enum ServingError {
	Sse(std::io::Error),
	StdIo(tokio::task::JoinError),
}

async fn serve(
	listener: Listener,
	state: Arc<RwLock<ProxyState>>,
) -> std::result::Result<(), ServingError> {
	match listener {
		Listener::Stdio {} => {
			let relay = serve_server(
				// TODO: This is a hack
				ServerHandlerService::new(Relay::new(state.clone(), rbac::Identity::empty())),
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
				.map_err(|e| ServingError::StdIo(e))
				.map(|_| ())
				.inspect_err(|e| {
					tracing::error!("serving error: {:?}", e);
				})
		},
		Listener::Sse { host, port, mode } => {
			let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port))
				.await
				.unwrap();
			let app = SseApp::new(state.clone());
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
				.map_err(|e| ServingError::Sse(e))
				.inspect_err(|e| {
					tracing::error!("serving error: {:?}", e);
				})
		},
	}
}
