use crate::types::discovery::Service;
use crate::types::discovery::{Endpoint, InboundProtocol, Workload};
use crate::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch::Sender;
use types::discovery::NamespacedHostname;
use types::discovery::NetworkAddress;

#[derive(Debug)]
pub struct Store {
	pub workloads: WorkloadStore,

	pub services: ServiceStore,
}
/// A WorkloadStore encapsulates all information about workloads in the mesh
#[derive(Debug)]
pub struct WorkloadStore {
	local_node: Option<Strng>,
	// TODO this could be expanded to Sender<Workload> + a full subscriber/streaming
	// model, but for now just notifying watchers to wake when _any_ insert happens
	// is simpler (and only requires a channelsize of 1)
	insert_notifier: Sender<()>,

	/// by_addr maps workload network addresses to workloads
	by_addr: HashMap<NetworkAddress, WorkloadByAddr>,
	/// by_uid maps workload UIDs to workloads
	pub(super) by_uid: HashMap<Strng, Arc<Workload>>,
}

impl WorkloadStore {
	pub fn find_uid(&self, uid: &Strng) -> Option<Arc<Workload>> {
		self.by_uid.get(uid).cloned()
	}

	/// Finds the workload by address, as an arc.
	pub fn find_address(&self, addr: &NetworkAddress) -> Option<Arc<Workload>> {
		self.by_addr.get(addr).map(WorkloadByAddr::get)
	}
}

/// Data store for service information.
#[derive(Default, Debug)]
pub struct ServiceStore {
	/// Maintains a mapping of service key -> (endpoint UID -> workload endpoint)
	/// this is used to handle ordering issues if workloads are received before services.
	pub(super) staged_services: HashMap<NamespacedHostname, HashMap<Strng, Endpoint>>,

	/// Allows for lookup of services by network address, the service's xds secondary key.
	pub(super) by_vip: HashMap<NetworkAddress, Arc<Service>>,

	/// Allows for lookup of services by hostname, and then by namespace. XDS uses a combination
	/// of hostname and namespace as the primary key. In most cases, there will be a single
	/// service for a given hostname. However, `ServiceEntry` allows hostnames to be overridden
	/// on a per-namespace basis.
	pub(super) by_host: HashMap<Strng, Vec<Arc<Service>>>,
}

impl ServiceStore {
	/// Returns the [Service] matching the given VIP.
	pub fn get_by_vip(&self, vip: &NetworkAddress) -> Option<Arc<Service>> {
		self.by_vip.get(vip).cloned()
	}
	pub fn get_by_namespaced_host(&self, host: &NamespacedHostname) -> Option<Arc<Service>> {
		// Get the list of services that match the hostname. Typically there will only be one, but
		// ServiceEntry allows configuring arbitrary hostnames on a per-namespace basis.
		match self.by_host.get(&host.hostname) {
			None => None,
			Some(services) => {
				// Return the service that matches the requested namespace.
				for service in services {
					if service.namespace == host.namespace {
						return Some(service.clone());
					}
				}
				None
			},
		}
	}
}

#[derive(Debug)]
/// WorkloadByAddr is a small wrapper around a single or multiple Workloads
/// We split these as in the vast majority of cases there is only a single one, so we save vec allocation.
enum WorkloadByAddr {
	Single(Arc<Workload>),
	Many(Vec<Arc<Workload>>),
}

impl WorkloadByAddr {
	// insert adds the workload
	pub fn insert(&mut self, w: Arc<Workload>) {
		match self {
			WorkloadByAddr::Single(workload) => {
				*self = WorkloadByAddr::Many(vec![workload.clone(), w]);
			},
			WorkloadByAddr::Many(v) => {
				v.push(w);
			},
		}
	}
	// remove_uid mutates the address to remove the workload referenced by the UID.
	// If 'true' is returned, there is no workload remaining at all
	pub fn remove_uid(&mut self, uid: Strng) -> bool {
		match self {
			WorkloadByAddr::Single(wl) => {
				// Remove it if the UID matches, else do nothing
				wl.uid == uid
			},
			WorkloadByAddr::Many(ws) => {
				ws.retain(|w| w.uid != uid);
				match ws.as_slice() {
					[] => true,
					[wl] => {
						// We now have one workload, transition to Single
						*self = WorkloadByAddr::Single(wl.clone());
						false
					},
					// We still have many. We removed already so no need to do anything
					_ => false,
				}
			},
		}
	}
	pub fn get(&self) -> Arc<Workload> {
		match self {
			WorkloadByAddr::Single(workload) => workload.clone(),
			WorkloadByAddr::Many(workloads) => workloads
				.iter()
				.max_by_key(|w| {
					// Setup a ranking criteria in the event of a conflict.
					// We prefer pod objects, as they are not (generally) spoof-able and is the most
					// likely to truthfully correspond to what is behind the service.
					let is_pod = w.uid.contains("//Pod/");
					// We fallback to looking for HBONE -- a resource marked as in the mesh is likely
					// to have more useful context than one not in the mesh.
					let is_hbone = w.protocol == InboundProtocol::HBONE;
					match (is_pod, is_hbone) {
						(true, true) => 3,
						(true, false) => 2,
						(false, true) => 1,
						(false, false) => 0,
					}
				})
				.expect("must have at least one workload")
				.clone(),
		}
	}
}
