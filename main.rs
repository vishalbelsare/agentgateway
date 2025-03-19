use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::get,
    Json, Router,
};
use clap::Parser;
use futures::{stream::Stream, SinkExt, StreamExt};
use rmcp::{
    model::*, serve_client, serve_server, service::RunningService,
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
mod relay;

use relay::Relay;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Sets a custom config file
    #[arg(short, long, value_name = "file")]
    filename: Option<std::path::PathBuf>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Config {
    input: Input,
    outputs: HashMap<String, Output>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input: Input::Stdio,
            outputs: HashMap::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Output {
    #[serde(rename = "sse")]
    Sse { host: String, port: u16 },
    #[serde(rename = "stdio")]
    Stdio { cmd: String, args: Vec<String> },
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Input {
    #[serde(rename = "sse")]
    Sse { host: String, port: u16 },
    #[serde(rename = "stdio")]
    Stdio,
}

type SessionId = Arc<str>;

fn session_id() -> SessionId {
    let id = format!("{:016x}", rand::random::<u128>());
    Arc::from(id)
}

#[derive(Clone, Default)]
pub struct App {
    services: HashMap<String, Arc<Mutex<RunningService<ClientHandlerService>>>>,
    txs: Arc<
        tokio::sync::RwLock<HashMap<SessionId, tokio::sync::mpsc::Sender<ClientJsonRpcMessage>>>,
    >,
}

impl App {
    pub fn new() -> Self {
        Self {
            txs: Default::default(),
            services: Default::default(),
        }
    }
    pub fn router(&self) -> Router {
        Router::new()
            .route("/sse", get(sse_handler).post(post_event_handler))
            .with_state(self.clone())
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostEventQuery {
    pub session_id: String,
}

async fn post_event_handler(
    State(app): State<App>,
    Query(PostEventQuery { session_id }): Query<PostEventQuery>,
    Json(message): Json<ClientJsonRpcMessage>,
) -> Result<StatusCode, StatusCode> {
    tracing::info!(session_id, ?message, "new client message");
    let tx = {
        let rg = app.txs.read().await;
        rg.get(session_id.as_str())
            .ok_or(StatusCode::NOT_FOUND)?
            .clone()
    };
    if tx.send(message).await.is_err() {
        tracing::error!("send message error");
        return Err(StatusCode::GONE);
    }
    Ok(StatusCode::ACCEPTED)
}

async fn sse_handler(State(app): State<App>) -> Sse<impl Stream<Item = Result<Event, io::Error>>> {
    // it's 4KB
    let session = session_id();
    tracing::info!(%session, "sse connection");
    use tokio_stream::wrappers::ReceiverStream;
    use tokio_util::sync::PollSender;
    let (from_client_tx, from_client_rx) = tokio::sync::mpsc::channel(64);
    let (to_client_tx, to_client_rx) = tokio::sync::mpsc::channel(64);
    app.txs
        .write()
        .await
        .insert(session.clone(), from_client_tx);
    {
        let session = session.clone();
        tokio::spawn(async move {
            let service = ServerHandlerService::new(Relay {
                services: app.services,
            });
            let stream = ReceiverStream::new(from_client_rx);
            let sink = PollSender::new(to_client_tx).sink_map_err(std::io::Error::other);
            let result = serve_server(service, (sink, stream))
                .await
                .inspect_err(|e| {
                    tracing::error!("serving error: {:?}", e);
                });

            if let Err(e) = result {
                tracing::error!(error = ?e, "initialize error");
                app.txs.write().await.remove(&session);
                return;
            }
            let _running_result = result.unwrap().waiting().await.inspect_err(|e| {
                tracing::error!(error = ?e, "running error");
            });
            app.txs.write().await.remove(&session);
        });
    }

    let stream = futures::stream::once(futures::future::ok(
        Event::default()
            .event("endpoint")
            .data(format!("?sessionId={session}")),
    ))
    .chain(ReceiverStream::new(to_client_rx).map(|message| {
        match serde_json::to_string(&message) {
            Ok(bytes) => Ok(Event::default().event("message").data(&bytes)),
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
        }
    }));
    Sse::new(stream)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    // Initialize the tracing subscriber with file and stdout logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::ERROR.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let args = Args::parse();
    let cfg = match args.filename {
        Some(filename) => {
            let file = std::fs::File::open(filename)?;
            let reader = std::io::BufReader::new(file);
            serde_json::from_reader(reader)?
        }
        None => {
            let mut cfg = Config::default();
            cfg.outputs.insert(
                "git".to_string(),
                Output::Stdio {
                    cmd: "uvx".to_string(),
                    args: vec!["mcp-server-git".to_string()],
                },
            );
            cfg.outputs.insert(
                "everything".to_string(),
                Output::Stdio {
                    cmd: "npx".to_string(),
                    args: vec![
                        "-y".to_string(),
                        "@modelcontextprotocol/server-everything".to_string(),
                    ],
                },
            );
            cfg
        }
    };

    let mut servers = JoinSet::new();
    for (name, output) in cfg.outputs.into_iter() {
        match output {
            Output::Stdio { cmd, args } => {
                tracing::info!("Starting stdio server: {name}");
                let client = serve_client(
                    ClientHandlerService::simple(),
                    TokioChildProcess::new(Command::new(cmd).args(args)).unwrap(),
                )
                .await
                .unwrap();
                tracing::info!("Connected to stdio server: {name}");
                servers.spawn(async move { (name, client) });
            }
            Output::Sse { host, port } => {
                tracing::info!("Starting sse server: {name}");
                let transport: SseTransport = SseTransport::start(
                    format!("http://{}:{}/sse", host, port).as_str(),
                    Default::default(),
                )
                .await
                .unwrap();

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
        let (name, client) = result.unwrap();
        tracing::info!("Server {name} exited");
        services.insert(name.to_string(), Arc::new(Mutex::new(client)));
    }

    // Create an instance of our counter router
    match cfg.input {
        Input::Stdio => {
            let relay = serve_server(
                ServerHandlerService::new(Relay { services: services }),
                (tokio::io::stdin(), tokio::io::stdout()),
            )
            .await
            .inspect_err(|e| {
                tracing::error!("serving error: {:?}", e);
            })?;
            relay.waiting().await?;
        }
        Input::Sse { host, port } => {
            let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port)).await?;
            let mut app = App::new();
            app.services = services;
            let router = app.router();
            axum::serve(listener, router).await?;
        }
    };

    Ok(())
}
