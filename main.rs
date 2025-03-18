use anyhow::Result;
use rmcp::service::RunningService;
use rmcp::{
    model::CallToolRequestParam, model::*, serve_client, serve_server, service::RequestContext,
    transport::child_process::TokioChildProcess, ClientHandlerService, Error as McpError,
    RoleServer, ServerHandler, ServerHandlerService, model::Tool, transport::sse::SseTransport
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing_subscriber::{self, EnvFilter};
use clap::Parser;
use serde::{Serialize, Deserialize};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Sets a custom config file
    #[arg(short, long, value_name = "file")]
    filename: Option<std::path::PathBuf>,
}

#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Debug)]
pub enum Output {
  Sse {
    host: String,
    port: u16,
  },
  Stdio {
    cmd: String,
    args: Vec<String>,
  }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Input {
  Sse {
    host: String,
    port: u16,
  },
  Stdio,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let cfg = match args.filename {
        Some(filename) => {
            let file = std::fs::File::open(filename)?;
            let reader = std::io::BufReader::new(file);
            serde_json::from_reader(reader)?
        }
        None => {
            let mut cfg = Config::default();
            cfg.outputs.insert("git".to_string(), Output::Stdio { cmd: "uvx".to_string(), args: vec!["mcp-server-git".to_string()] });
            cfg.outputs.insert("everything".to_string(), Output::Stdio { cmd: "npx".to_string(), args: vec!["-y".to_string(), "@modelcontextprotocol/server-everything".to_string()] });
            cfg
        }
    };

    let servers = futures::future::join_all(cfg.outputs.iter().map(async |(name, output)| {
        match output {
            Output::Stdio { cmd, args } => {
              tracing::info!("Starting stdio server: {name}");
              let client = serve_client(
                  ClientHandlerService::simple(),
                  TokioChildProcess::new(Command::new(cmd).args(args)).unwrap(),
              )
              .await.unwrap();
              tracing::info!("Connected to stdio server: {name}");
              (name, client)
            }
            Output::Sse { host, port } => {
              tracing::info!("Starting sse server: {name}");
              let transport: SseTransport = SseTransport::start(format!("http://{}:{}/sse", host, port).as_str(), Default::default()).await.unwrap();

              let client = serve_client(ClientHandlerService::simple(), transport)
                  .await
                  .inspect_err(|e| {
                      tracing::error!("client error: {:?}", e);
                  }).unwrap();
              tracing::info!("Connected to sse server: {name}");
              (name, client)
            }
        }
    })).await.into_iter().collect::<HashMap<&String, RunningService<ClientHandlerService>>>();

    // Initialize logging
    // Initialize the tracing subscriber with file and stdout logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::ERROR.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    // Create an instance of our counter router
    let relay = serve_server(
        ServerHandlerService::new(Relay {
            services: servers.into_iter().map(|(name, client)| (name.to_string(), Arc::new(Mutex::new(client)))).collect(),
        }),
        (tokio::io::stdin(), tokio::io::stdout()),
    )
    .await
    .inspect_err(|e| {
        tracing::error!("serving error: {:?}", e);
    })?;

    relay.waiting().await?;
    Ok(())
}

#[derive(Clone)]
pub struct Relay {
    services: HashMap<String, Arc<Mutex<RunningService<ClientHandlerService>>>>,
}

impl ServerHandler for Relay {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities {
                experimental: None,
                logging: None,
                prompts: Some(PromptsCapability::default()),
                resources: Some(ResourcesCapability::default()),
                tools: Some(ToolsCapability {
                    list_changed: None,
                }),
            },
            server_info: Implementation::from_build_env(),
            instructions: Some("This server provides a counter tool that can increment and decrement values. The counter starts at 0 and can be modified using the 'increment' and 'decrement' tools. Use 'get_value' to check the current count.".to_string()),
        }
    }

    async fn list_tools(
        &self,
        request: PaginatedRequestParam,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, McpError> {
        let mut tools = Vec::new();
        for (name, service) in self.services.iter() {
            let result = service.as_ref().lock().await.list_tools(request.clone()).await.unwrap();
            for tool in result.tools {
                let tool_name = format!("{}:{}", name, tool.name);
                tools.push(Tool {
                    name: Cow::Owned(tool_name.into()),
                    description: tool.description,
                    input_schema: tool.input_schema,
                });
            }
        }
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }


    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, McpError> {
        let tool_name = request.name.to_string();
        let (service_name, tool) = tool_name.split_once(':').unwrap();
        let service = self.services.get(service_name).unwrap();
        let req = CallToolRequestParam {
            name: Cow::Owned(tool.to_string()),
            arguments: request.arguments,
        };
        
        let result = service.as_ref().lock().await.call_tool(req).await.unwrap();
        Ok(result)
    }

}
