use std::sync::Arc;
use tracing::{debug, info, trace};

use rmcp::{ServerHandlerService, serve_server};

use crate::proxyprotocol;
use crate::rbac;
use crate::relay::Relay;
use crate::sse::App as SseApp;
use crate::state::{Listener, ListenerMode, State as ProxyState, Target};
use axum::http::HeaderMap;

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

pub async fn run_local_client(cfg: StaticConfig) -> Result<(), anyhow::Error> {
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
	info!(%num_targets, %num_policies, "local config initialized");
	serve(cfg.listener, state).await
}

async fn serve(listener: Listener, state: ProxyState) -> Result<(), anyhow::Error> {
	match listener {
		Listener::Stdio {} => {
			let relay = serve_server(
				// TODO: This is a hack
				ServerHandlerService::new(Relay::new(Arc::new(state), rbac::Identity::empty())),
				(tokio::io::stdin(), tokio::io::stdout()),
			)
			.await
			.inspect_err(|e| {
				tracing::error!("serving error: {:?}", e);
			})?;
			relay.waiting().await?;
		},
		Listener::Sse { host, port, mode } => {
			info!("serving sse on {}:{}", host, port);
			let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port)).await?;
			let app = SseApp::new(Arc::new(state));
			let router = app.router();

			let enable_proxy = Some(&ListenerMode::Proxy) == mode.as_ref();

			let listener = proxyprotocol::Listener::new(listener, enable_proxy);
			let svc = router.into_make_service_with_connect_info::<proxyprotocol::Address>();
			axum::serve(listener, svc).await?;
		},
	};

	Ok(())
}
