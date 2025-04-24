use crate::*;
use anyhow::anyhow;
use prometheus_client::encoding::{EncodeLabelValue, LabelValueEncoder};
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Formatter, Write};
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Debug, Eq, PartialEq, Hash, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NamespacedHostname {
	pub namespace: Strng,
	pub hostname: Strng,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct NetworkAddress {
	pub network: Strng,
	pub address: IpAddr,
}

// we need custom serde serialization since NetworkAddress is keying maps
impl Serialize for NetworkAddress {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.collect_str(&self)
	}
}

impl fmt::Display for NetworkAddress {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&self.network)?;
		f.write_char('/')?;
		f.write_str(&self.address.to_string())
	}
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Workload {
	pub workload_ips: Vec<IpAddr>,

	#[serde(default, skip_serializing_if = "is_default")]
	pub waypoint: Option<GatewayAddress>,
	#[serde(default, skip_serializing_if = "is_default")]
	pub network_gateway: Option<GatewayAddress>,

	#[serde(default)]
	pub protocol: InboundProtocol,
	#[serde(default)]
	pub network_mode: NetworkMode,

	#[serde(default, skip_serializing_if = "is_default")]
	pub uid: Strng,
	#[serde(default)]
	pub name: Strng,
	pub namespace: Strng,
	#[serde(default, skip_serializing_if = "is_default")]
	pub trust_domain: Strng,
	#[serde(default, skip_serializing_if = "is_default")]
	pub service_account: Strng,
	#[serde(default, skip_serializing_if = "is_default")]
	pub network: Strng,

	#[serde(default, skip_serializing_if = "is_default")]
	pub workload_name: Strng,
	#[serde(default, skip_serializing_if = "is_default")]
	pub workload_type: Strng,
	#[serde(default, skip_serializing_if = "is_default")]
	pub canonical_name: Strng,
	#[serde(default, skip_serializing_if = "is_default")]
	pub canonical_revision: Strng,

	#[serde(default, skip_serializing_if = "is_default")]
	pub hostname: Strng,

	#[serde(default, skip_serializing_if = "is_default")]
	pub node: Strng,

	#[serde(default, skip_serializing_if = "is_default")]
	pub authorization_policies: Vec<Strng>,

	#[serde(default)]
	pub status: HealthStatus,

	#[serde(default)]
	pub cluster_id: Strng,

	#[serde(default, skip_serializing_if = "is_default")]
	pub locality: Locality,

	#[serde(default, skip_serializing_if = "is_default")]
	pub services: Vec<NamespacedHostname>,

	pub capacity: u32,
}

impl Workload {
	pub fn identity(&self) -> Identity {
		Identity::Spiffe {
			trust_domain: self.trust_domain.clone(),
			namespace: self.namespace.clone(),
			service_account: self.service_account.clone(),
		}
	}
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub enum Identity {
	Spiffe { trust_domain: Strng, namespace: Strng, service_account: Strng },
}

impl EncodeLabelValue for Identity {
	fn encode(&self, writer: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
		writer.write_str(&self.to_string())
	}
}

impl serde::Serialize for Identity {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		self.to_string().serialize(serializer)
	}
}

impl FromStr for Identity {
	type Err = anyhow::Error;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		const URI_PREFIX: &str = "spiffe://";
		const SERVICE_ACCOUNT: &str = "sa";
		const NAMESPACE: &str = "ns";
		if !s.starts_with(URI_PREFIX) {
			return Err(anyhow!("invalid spiffe: {s}"));
		}
		let split: Vec<_> = s[URI_PREFIX.len()..].split('/').collect();
		if split.len() != 5 {
			return Err(anyhow!("invalid spiffe: {s}"));
		}
		if split[1] != NAMESPACE || split[3] != SERVICE_ACCOUNT {
			return Err(anyhow!("invalid spiffe: {s}"));
		}
		Ok(Identity::Spiffe {
			trust_domain: split[0].into(),
			namespace: split[2].into(),
			service_account: split[4].into(),
		})
	}
}

impl Display for Identity {
	fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
		match self {
			Identity::Spiffe { trust_domain, namespace, service_account } => {
				write!(f, "spiffe://{trust_domain}/ns/{namespace}/sa/{service_account}")
			}
		}
	}
}

impl Identity {
	pub fn from_parts(td: Strng, ns: Strng, sa: Strng) -> Identity {
		Identity::Spiffe { trust_domain: td, namespace: ns, service_account: sa }
	}

	pub fn to_strng(self: &Identity) -> Strng {
		match self {
			Identity::Spiffe { trust_domain, namespace, service_account } => {
				strng::format!("spiffe://{trust_domain}/ns/{namespace}/sa/{service_account}")
			}
		}
	}

	pub fn trust_domain(&self) -> Strng {
		match self {
			Identity::Spiffe { trust_domain, .. } => trust_domain.clone(),
		}
	}
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
	*t == Default::default()
}

// The protocol that the sender should use to send data. Can be different from ServerProtocol when there is a
// proxy in the middle (e.g. e/w gateway with double hbone).
#[derive(Default, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum OutboundProtocol {
	#[default]
	TCP,
	HBONE,
	DOUBLEHBONE,
}

#[derive(Default, Debug, Hash, Eq, PartialEq, Clone, Copy, serde::Serialize)]
pub enum NetworkMode {
	#[default]
	Standard,
	HostNetwork,
}

#[derive(Default, Debug, Hash, Eq, PartialEq, Clone, Copy, serde::Serialize)]
pub enum HealthStatus {
	#[default]
	Healthy,
	Unhealthy,
}

#[derive(Default, Debug, Hash, Eq, PartialEq, Clone, serde::Serialize)]
pub struct Locality {
	pub region: Strng,
	pub zone: Strng,
	pub subzone: Strng,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayAddress {
	pub destination: gatewayaddress::Destination,
	pub hbone_mtls_port: u16,
}

pub mod gatewayaddress {
	use super::{NamespacedHostname, NetworkAddress};
	#[derive(Debug, Hash, Eq, PartialEq, Clone, serde::Serialize)]
	#[serde(untagged)]
	pub enum Destination {
		Address(NetworkAddress),
		Hostname(NamespacedHostname),
	}
}

// The protocol that the final workload expects
#[derive(Default, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum InboundProtocol {
	#[default]
	TCP,
	HBONE,
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Service {
	pub name: Strng,
	pub namespace: Strng,
	pub hostname: Strng,
	pub vips: Vec<NetworkAddress>,
	pub ports: HashMap<u16, u16>,

	pub app_protocols: HashMap<u16, AppProtocol>,

	/// Maps endpoint UIDs to service [Endpoint]s.
	#[serde(default)]
	pub endpoints: EndpointSet,
	#[serde(default)]
	pub subject_alt_names: Vec<Strng>,

	#[serde(default, skip_serializing_if = "is_default")]
	pub waypoint: Option<GatewayAddress>,

	#[serde(default, skip_serializing_if = "is_default")]
	pub load_balancer: Option<LoadBalancer>,

	#[serde(default, skip_serializing_if = "is_default")]
	pub ip_families: Option<IpFamily>,
}

impl Service {
	pub fn port_is_http2(&self, port: u16) -> bool {
		matches!(self.app_protocols.get(&port), Some(AppProtocol::Http2 | AppProtocol::Grpc))
	}
	pub fn namespaced_hostname(&self) -> NamespacedHostname {
		NamespacedHostname { namespace: self.namespace.clone(), hostname: self.hostname.clone() }
	}
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, serde::Serialize)]
pub enum AppProtocol {
	Http11,
	Http2,
	Grpc,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, serde::Serialize)]
pub enum IpFamily {
	Dual,
	IPv4,
	IPv6,
}

impl IpFamily {
	/// accepts_ip returns true if the provided IP is supposed by the IP family
	pub fn accepts_ip(&self, ip: IpAddr) -> bool {
		match self {
			IpFamily::Dual => true,
			IpFamily::IPv4 => ip.is_ipv4(),
			IpFamily::IPv6 => ip.is_ipv6(),
		}
	}
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadBalancer {
	pub routing_preferences: Vec<LoadBalancerScopes>,
	pub mode: LoadBalancerMode,
	pub health_policy: LoadBalancerHealthPolicy,
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize)]
pub enum LoadBalancerScopes {
	Region,
	Zone,
	Subzone,
	Node,
	Cluster,
	Network,
}

#[derive(Default, Debug, Eq, PartialEq, Clone, serde::Serialize)]
pub enum LoadBalancerHealthPolicy {
	#[default]
	OnlyHealthy,
	AllowAll,
}
#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize)]
pub enum LoadBalancerMode {
	// Do not consider LoadBalancerScopes when picking endpoints
	Standard,
	// Only select endpoints matching all LoadBalancerScopes when picking endpoints; otherwise, fail.
	Strict,
	// Prefer select endpoints matching all LoadBalancerScopes when picking endpoints but allow mismatches
	Failover,
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Endpoint {
	/// The workload UID for this endpoint.
	pub workload_uid: Strng,

	/// The port mapping.
	pub port: HashMap<u16, u16>,

	/// Health status for the endpoint
	pub status: HealthStatus,
}

/// EndpointSet is an abstraction over a set of endpoints.
/// While this is currently not very useful, merely wrapping a HashMap, the intent is to make this future
/// proofed to future enhancements, such as keeping track of load balancing information the ability
/// to incrementally update.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct EndpointSet {
	pub inner: HashMap<Strng, Arc<Endpoint>>,
}

impl serde::Serialize for EndpointSet {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		self.inner.serialize(serializer)
	}
}

impl EndpointSet {
	pub fn from_list<const N: usize>(eps: [Endpoint; N]) -> EndpointSet {
		let mut endpoints = HashMap::with_capacity(eps.len());
		for ep in eps.into_iter() {
			endpoints.insert(ep.workload_uid.clone(), Arc::new(ep));
		}
		EndpointSet { inner: endpoints }
	}

	pub fn insert(&mut self, k: Strng, v: Endpoint) {
		self.inner.insert(k, Arc::new(v));
	}

	pub fn contains(&self, key: &Strng) -> bool {
		self.inner.contains_key(key)
	}

	pub fn get(&self, key: &Strng) -> Option<&Endpoint> {
		self.inner.get(key).map(Arc::as_ref)
	}

	pub fn remove(&mut self, key: &Strng) {
		self.inner.remove(key);
	}

	pub fn iter(&self) -> impl Iterator<Item = &Endpoint> {
		self.inner.values().map(Arc::as_ref)
	}
}
