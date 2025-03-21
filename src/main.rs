use anyhow::Result;
use clap::Parser;
use mcp_gateway::state::{Listener, ListenerMode, Target, TargetSpec};
use prometheus_client::registry::Registry;
use rmcp::{
	ClientHandlerService, ServerHandlerService, serve_client, serve_server, service::RunningService,
	transport::child_process::TokioChildProcess, transport::sse::SseTransport,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing_subscriber::{self, EnvFilter};
use config::Config as XdsConfig;
use xds::LocalConfig;
#[allow(warnings)]
#[allow(clippy::derive_partial_eq_without_eq)]
mod proto {
	tonic::include_proto!("envoy.service.discovery.v3");
}

use mcp_gateway::metrics::App as MetricsApp;
use mcp_gateway::relay::Relay;
use mcp_gateway::sse::App as SseApp;
use mcp_gateway::*;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
	/// Use config from bytes
	#[arg(short, long, value_name = "config")]
	config: Option<bytes::Bytes>,

  /// Use config from file
  #[arg(short, long, value_name = "file")]
  file: Option<std::path::PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Config {
  #[serde(rename_all = "camelCase")]
  Local(LocalConfig),
  Xds(XdsConfig),
}

#[tokio::main]
async fn main() -> Result<()> {
	// Initialize logging
	// Initialize the tracing subscriber with file and stdout logging
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
		.with_writer(std::io::stderr)
		.with_ansi(false)
		.init();

	let registry = Registry::default();

	let args = Args::parse();

  // if args.file.is_none() && args.config.is_none() {
  //   eprintln!("Error: either --file or --config must be provided, exiting");
  //   std::process::exit(1);
  // }
  
  let cfg: Config = match (args.file, args.config) {
    (Some(filename), None) => {
      let file = tokio::fs::read_to_string(filename).await?;
      serde_yaml::from_str(&file)?
    },
    (None, Some(config)) => {
      let file = std::str::from_utf8(&config).map(|s| s.to_string())?;
      serde_yaml::from_str(&file)?
    },
    (Some(_), Some(_)) => {
      eprintln!("config error: both --file and --config cannot be provided, exiting");
      std::process::exit(1);
    },
    (None, None) => {
      eprintln!("Error: either --file or --config must be provided, exiting");
      std::process::exit(1);
    },
  };


	let mut servers = JoinSet::new();
	for (name, output) in cfg.targets.into_iter() {
		match output {
			Target::Stdio { cmd, args } => {
				tracing::info!("Starting stdio server: {name}");
				let client = serve_client(
					ClientHandlerService::simple(),
					TokioChildProcess::new(Command::new(cmd).args(args))?,
				)
				.await?;
				tracing::info!("Connected to stdio server: {name}");
				servers.spawn(async move { (name, client) });
			},
			Target::Sse { host, port } => {
				tracing::info!("Starting sse server: {name}");
				let transport: SseTransport = SseTransport::start(
					format!("http://{}:{}/sse", host, port).as_str(),
					Default::default(),
				)
				.await?;

				let client = serve_client(ClientHandlerService::simple(), transport)
					.await
					.inspect_err(|e| {
						tracing::error!("client error: {:?}", e);
					})
					.unwrap();
				tracing::info!("Connected to sse server: {name}");
				servers.spawn(async move { (name, client) });
			},
		}
	}

	let mut services: HashMap<String, Arc<Mutex<RunningService<ClientHandlerService>>>> =
		HashMap::new();
	while let Some(result) = servers.join_next().await {
		let (name, client) = result?;
		services.insert(name.to_string(), Arc::new(Mutex::new(client)));
	}

	let mut run_set = JoinSet::new();

	// Create an instance of our counter router
	match cfg.listener.unwrap_or_default() {
		Listener::Stdio {} => {
			let relay = serve_server(
				ServerHandlerService::new(Relay {
					services,
					rbac: rbac::RbacEngine::passthrough(),
				}),
				(tokio::io::stdin(), tokio::io::stdout()),
			)
			.await
			.inspect_err(|e| {
				tracing::error!("serving error: {:?}", e);
			})?;
			relay.waiting().await?;
		},
		Listener::Sse { host, port, mode } => {
			let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port)).await?;
			let app = SseApp::new();
			let router = app.router();

			let enable_proxy = Some(ListenerMode::Proxy) == mode;

			let listener = proxyprotocol::Listener::new(listener, enable_proxy);
			let svc = router.into_make_service_with_connect_info::<proxyprotocol::Address>();
			run_set.spawn(async move {
				axum::serve(listener, svc).await;
			});
		},
	};

	// Add metrics listener
	let listener = tokio::net::TcpListener::bind("0.0.0.0:19000").await?;
	let app = MetricsApp::new(Arc::new(registry));
	let router = app.router();
	run_set.spawn(async move {
		axum::serve(listener, router).await;
	});

	// Wait for all servers to finish? I think this does what I want :shrug:
	while let Some(result) = run_set.join_next().await {
		result?;
	}
	Ok(())
}
