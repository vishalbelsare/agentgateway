use std::collections::HashMap;

use crate::rbac;
use crate::xds::mcp::kgateway_dev::listener::Listener as XdsListener;
use crate::xds::mcp::kgateway_dev::target::Target as XdsTarget;
use rmcp::ClientHandlerService;
use rmcp::serve_client;
use rmcp::service::RunningService;
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::transport::sse::SseTransport;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct Target {
	pub name: String,
	pub spec: TargetSpec,
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum TargetSpec {
	#[serde(rename = "sse")]
	Sse { host: String, port: u32 },
	#[serde(rename = "stdio")]
	Stdio { cmd: String, args: Vec<String> },
}

impl From<&XdsTarget> for Target {
	fn from(value: &XdsTarget) -> Self {
		Target {
			name: value.name.clone(),
			spec: {
				TargetSpec::Sse {
					host: value.host.clone(),
					port: value.port,
				}
			},
		}
	}
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum Listener {
	#[serde(rename = "sse")]
	Sse {
		host: String,
		port: u32,
		mode: Option<ListenerMode>,
	},
	#[serde(rename = "stdio")]
	Stdio {},
}

impl From<&XdsListener> for Listener {
	fn from(value: &XdsListener) -> Self {
		Listener::Sse {
			host: value.host.clone(),
			port: value.port,
			mode: None,
		}
	}
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum ListenerMode {
	#[serde(rename = "proxy")]
	Proxy,
}

impl Default for Listener {
	fn default() -> Self {
		Self::Stdio {}
	}
}

pub struct ConnectionPool {
	demand_rx: mpsc::Receiver<(oneshot::Sender<()>, Target)>,
	by_name: HashMap<String, Arc<RwLock<RunningService<ClientHandlerService>>>>,
}

impl ConnectionPool {
	pub fn new(demand_rx: mpsc::Receiver<(oneshot::Sender<()>, Target)>) -> Self {
		Self {
			demand_rx,
			by_name: HashMap::new(),
		}
	}

	// This watches for new targets and starts connections to them
	pub async fn run(&mut self) -> Result<(), anyhow::Error> {
		loop {
			match self.demand_rx.recv().await {
				Some((tx, target)) => {
					let transport: RunningService<ClientHandlerService> = match target.spec {
						TargetSpec::Sse { host, port } => {
							let transport: SseTransport = SseTransport::start(
								format!("http://{}:{}", host, port).as_str(),
								Default::default(),
							)
							.await?;
							serve_client(ClientHandlerService::simple(), transport).await?
						},
						TargetSpec::Stdio { cmd, args } => {
							serve_client(
								ClientHandlerService::simple(),
								TokioChildProcess::new(Command::new(cmd).args(args)).unwrap(),
							)
							.await?
						},
					};
					let connection = Arc::new(RwLock::new(transport));
					self.by_name.insert(target.name.clone(), connection.clone());
					tx.send(()).unwrap();
				},
				None => {
					tracing::error!("Connection pool receiver closed");
					return Err(anyhow::anyhow!("Connection pool receiver closed"));
				},
			}
		}
	}

	pub fn get(&self, name: &str) -> Option<&Arc<RwLock<RunningService<ClientHandlerService>>>> {
		self.by_name.get(name)
	}
}

pub struct TargetStore {
	by_name: HashMap<String, Target>,
	demand: mpsc::Sender<(oneshot::Sender<()>, Target)>,
	connections: ConnectionPool,
}

impl TargetStore {
	pub fn new() -> Self {
		let (demand, demand_rx) = mpsc::channel(100);
		let connections = ConnectionPool::new(demand_rx);
		Self {
			by_name: HashMap::new(),
			demand,
			connections,
		}
	}

	pub fn remove(&mut self, name: &str) {
		// TODO: Drain connections from target
		self.by_name.remove(name);
	}

	pub fn insert(&mut self, target: Target) {
		self.by_name.insert(target.name.clone(), target);
	}

	pub async fn get(
		&self,
		name: &str,
	) -> Option<&Arc<RwLock<RunningService<ClientHandlerService>>>> {
		match self.connections.get(name) {
			Some(connection) => Some(connection),
			None => {
				// TODO: Handle error
				let (tx, rx) = oneshot::channel();
				// Send demand for connection
				self
					.demand
					.send((tx, self.by_name[name].clone()))
					.await
					.unwrap();
				// Wait for connection
				match rx.await {
					Ok(_) => self.connections.get(name),
					Err(_) => {
						tracing::error!("Connection not found for target: {}", name);
						None
					},
				}
			},
		}
	}

	pub async fn iter(
		&self,
	) -> impl Iterator<Item = (String, &Arc<RwLock<RunningService<ClientHandlerService>>>)> {
		let x = self
			.by_name
			.iter()
			.map(|(name, _)| async move { (name.clone(), self.get(name).await) });

		futures::future::join_all(x)
			.await
			.into_iter()
			.filter(|(_, connection)| connection.is_some())
			.map(|(name, connection)| (name, connection.unwrap()))
	}

	pub fn clear(&mut self) {
		self.by_name.clear();
	}
}

pub struct PolicyStore {
	by_name: HashMap<String, rbac::RuleSet>,
}

impl PolicyStore {
	pub fn new() -> Self {
		Self {
			by_name: HashMap::new(),
		}
	}
}

impl PolicyStore {
	pub fn insert(&mut self, policy: rbac::RuleSet) {
		self.by_name.insert(policy.to_key(), policy);
	}

	pub fn remove(&mut self, name: &str) {
		self.by_name.remove(name);
	}

	pub fn validate(&self, resource: &rbac::ResourceType, claims: &rbac::Claims) -> bool {
		self
			.by_name
			.values()
			.into_iter()
			.any(|policy| policy.validate(resource, claims))
	}

	pub fn clear(&mut self) {
		self.by_name.clear();
	}
}

pub struct State {
	pub targets: TargetStore,
	pub policies: PolicyStore,
}

impl State {
	pub fn new() -> Self {
		Self {
			targets: TargetStore::new(),
			policies: PolicyStore::new(),
		}
	}
}
