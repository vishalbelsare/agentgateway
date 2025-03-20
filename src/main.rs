use anyhow::Result;
use clap::Parser;
use rmcp::{
    serve_client, serve_server, service::RunningService,
    transport::child_process::TokioChildProcess, transport::sse::SseTransport,
    ClientHandlerService, ServerHandlerService,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{self};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing_subscriber::{self, EnvFilter};

#[allow(warnings)]
#[allow(clippy::derive_partial_eq_without_eq)]
mod proto {
    tonic::include_proto!("envoy.service.discovery.v3");
}

use mcp_gateway::relay::Relay;
use mcp_gateway::sse::App;
use mcp_gateway::*;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Sets a custom config file
    #[arg(short, long, value_name = "config")]
    config: Option<std::path::PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    listener: Option<Listener>,
    targets: HashMap<String, Targets>,
    rules: Vec<rbac::Rule>,
}

impl Config {
    pub fn new(outputs: HashMap<String, Targets>) -> Self {
        Self {
            listener: Some(Listener::Stdio{}),
            targets: outputs,
            rules: vec![],
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Targets {
    #[serde(rename = "sse")]
    Sse { host: String, port: u16 },
    #[serde(rename = "stdio")]
    Stdio { cmd: String, args: Vec<String> },
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Listener {
    #[serde(rename = "sse")]
    Sse { host: String, port: u16 },
    #[serde(rename = "stdio")]
    Stdio{},
}

impl Default for Listener {
    fn default() -> Self {
        Self::Stdio{}
    }
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

    let args = Args::parse();
    let cfg = match args.config {
        Some(filename) => {
            let file = std::fs::File::open(filename)?;
            let reader = std::io::BufReader::new(file);
            serde_json::from_reader(reader)?
        }
        None => {
            Config::new(HashMap::from([
                ("git".to_string(), Targets::Stdio {
                    cmd: "uvx".to_string(),
                    args: vec!["mcp-server-git".to_string()],
                }),
                ("everything".to_string(), Targets::Stdio {
                    cmd: "npx".to_string(),
                    args: vec![
                        "-y".to_string(),
                        "@modelcontextprotocol/server-everything".to_string(),
                    ],
                }),
            ]))
        }
    };

    let mut servers = JoinSet::new();
    for (name, output) in cfg.targets.into_iter() {
        match output {
            Targets::Stdio { cmd, args } => {
                tracing::info!("Starting stdio server: {name}");
                let client = serve_client(
                    ClientHandlerService::simple(),
                    TokioChildProcess::new(Command::new(cmd).args(args))?,
                )
                .await?;
                tracing::info!("Connected to stdio server: {name}");
                servers.spawn(async move { (name, client) });
            }
            Targets::Sse { host, port } => {
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
            }
        }
    }

    let mut services: HashMap<String, Arc<Mutex<RunningService<ClientHandlerService>>>> =
        HashMap::new();
    while let Some(result) = servers.join_next().await {
        let (name, client) = result?;
        services.insert(name.to_string(), Arc::new(Mutex::new(client)));
    }

    // Create an instance of our counter router
    match cfg.listener.unwrap_or_default() {
        Listener::Stdio{} => {
            let relay = serve_server(
                ServerHandlerService::new(Relay { services: services, rbac: rbac::RbacEngine::passthrough() }),
                (tokio::io::stdin(), tokio::io::stdout()),
            )
            .await
            .inspect_err(|e| {
                tracing::error!("serving error: {:?}", e);
            })?;
            relay.waiting().await?;
        }
        Listener::Sse { host, port } => {
            let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port)).await?;
            let app = App::new(services, cfg.rules);
            let router = app.router();
            axum::serve(listener, router).await?;
        }
    };

    Ok(())
}
