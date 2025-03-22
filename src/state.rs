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

pub struct TargetStore {
	by_name: HashMap<String, Target>,

	connections: HashMap<String, Arc<RwLock<RunningService<ClientHandlerService>>>>,
}

impl TargetStore {
	pub fn new() -> Self {
		Self {
			by_name: HashMap::new(),
			connections: HashMap::new(),
		}
	}

	// TODO: get rid of unwraps
	// TODO: Separate connection/LB from insertion
	pub async fn insert(&mut self, target: Target) {
		let name = target.name.clone();
		self.by_name.insert(target.name.clone(), target);
		match self.connections.get(&name) {
			Some(_) => {},
			None => match self.by_name.get(&name) {
				Some(target) => match target.spec.clone() {
					TargetSpec::Sse { host, port } => {
						let transport = SseTransport::start(
							format!("http://{}:{}", host, port).as_str(),
							Default::default(),
						)
						.await
						.unwrap();
						let client = serve_client(ClientHandlerService::simple(), transport)
							.await
							.unwrap();
						let connection: Arc<RwLock<RunningService<ClientHandlerService>>> =
							Arc::new(RwLock::new(client));
						self
							.connections
							.insert(name.to_string(), connection.clone());
					},
					TargetSpec::Stdio { cmd, args } => {
						tracing::info!("Starting stdio server: {name}");
						let client: RunningService<ClientHandlerService> = serve_client(
							ClientHandlerService::simple(),
							TokioChildProcess::new(Command::new(cmd).args(args)).unwrap(),
						)
						.await
						.unwrap();
						tracing::info!("Connected to stdio server: {name}");
						let connection = Arc::new(RwLock::new(client));
						self
							.connections
							.insert(name.to_string(), connection.clone());
					},
				},
				None => {
					panic!("Target not found");
				},
			},
		};
	}

	pub fn remove(&mut self, name: &str) {
		self.by_name.remove(name);
		self.connections.remove(name);
	}

	pub fn get(&self, name: &str) -> Option<&Target> {
		self.by_name.get(name)
	}

	pub fn get_connection(
		&self,
		name: &str,
	) -> Option<&Arc<RwLock<RunningService<ClientHandlerService>>>> {
		self.connections.get(name)
	}

	pub fn iter_connections(
		&self,
	) -> impl Iterator<Item = (String, &Arc<RwLock<RunningService<ClientHandlerService>>>)> {
		self
			.connections
			.iter()
			.map(|(name, connection)| (name.clone(), connection))
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

	pub fn get(&self, name: &str) -> Option<Vec<rbac::Rule>> {
		self
			.by_name
			.get(name)
			.map(|rule_set| rule_set.rules.clone())
	}

	pub fn iter(&self) -> impl Iterator<Item = &rbac::Rule> {
		self
			.by_name
			.values()
			.flat_map(|rule_set| rule_set.rules.iter())
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
