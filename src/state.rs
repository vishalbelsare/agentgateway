use crate::xds::mcp::kgateway_dev::listener::Listener as XdsListener;
use crate::xds::mcp::kgateway_dev::rbac::Config as XdsRbac;
use crate::xds::mcp::kgateway_dev::target::Target as XdsTarget;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Target {
	name: String,
	spec: TargetSpec,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
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

#[derive(Clone, Serialize, Deserialize, Debug)]
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
