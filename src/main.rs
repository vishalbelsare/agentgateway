use anyhow::Result;
use clap::Parser;
use mcp_gateway::config::Config as XdsConfig;
use mcp_gateway::r#static::{LocalConfig, run_local_client};
use prometheus_client::registry::Registry;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::task::JoinSet;
use tracing_subscriber::{self, EnvFilter};

use mcp_gateway::metrics::App as MetricsApp;

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
#[serde(tag = "type")]
pub enum Config {
	#[serde(rename = "local")]
	Local(LocalConfig),
	#[serde(rename = "xds")]
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
			serde_json::from_str(&file)?
		},
		(None, Some(config)) => {
			let file = std::str::from_utf8(&config).map(|s| s.to_string())?;
			serde_json::from_str(&file)?
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

	let local = match cfg {
		Config::Local(cfg) => cfg,
		Config::Xds(_) => {
			eprintln!("XDS config not supported yet");
			std::process::exit(1);
		},
	};

	let mut run_set = JoinSet::new();

	run_set.spawn(async move {
		run_local_client(local).await;
	});

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
