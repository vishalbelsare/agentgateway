use crate::types::discovery::{Endpoint, InboundProtocol, Workload};
use crate::types::discovery::{NetworkMode, Service};
use crate::*;
use agent_xds::XdsUpdate;
use itertools::Itertools;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::watch::Sender;
use tracing::Level;
use tracing::instrument;
use types::discovery::NamespacedHostname;
use types::discovery::NetworkAddress;
use types::proto::workload::Address as XdsAddress;
use types::proto::workload::PortList;
use types::proto::workload::Service as XdsService;
use types::proto::workload::Workload as XdsWorkload;
use types::proto::workload::address::Type as XdsType;

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
	pub fn insert(&mut self, w: Arc<Workload>) {
		// First, remove the entry entirely to make sure things are cleaned up properly.
		self.remove(&w.uid);

		if w.network_mode != NetworkMode::HostNetwork {
			for ip in &w.workload_ips {
				let k = network_addr(w.network.clone(), *ip);
				self
					.by_addr
					.entry(k)
					.and_modify(|ws| ws.insert(w.clone()))
					.or_insert_with(|| WorkloadByAddr::Single(w.clone()));
			}
		}
		self.by_uid.insert(w.uid.clone(), w.clone());

		// We have stored a newly inserted workload, notify watchers
		// (if any) to wake.
		self.insert_notifier.send_replace(());
	}

	fn remove(&mut self, uid: &Strng) -> Option<Workload> {
		match self.by_uid.remove(uid) {
			None => {
				trace!("tried to remove workload but it was not found");
				None
			},
			Some(prev) => {
				if prev.network_mode != NetworkMode::HostNetwork {
					for wip in prev.workload_ips.iter() {
						if let Entry::Occupied(mut o) =
							self.by_addr.entry(network_addr(prev.network.clone(), *wip))
						{
							if o.get_mut().remove_uid(prev.uid.clone()) {
								o.remove();
							}
						}
					}
				}

				Some(prev.deref().clone())
			},
		}
	}
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
	fn insert_endpoint_for_services(
		&mut self,
		workload: &Arc<Workload>,
		services: &HashMap<String, PortList>,
	) -> anyhow::Result<()> {
		for (namespaced_host, ports) in services {
			// Parse the namespaced hostname for the service.
			let namespaced_host = NamespacedHostname::from_str(namespaced_host)?;
			self.insert_endpoint(
				namespaced_host,
				Endpoint {
					workload_uid: workload.uid.clone(),
					port: ports.into(),
					status: workload.status,
				},
			)
		}
		Ok(())
	}
	fn insert_endpoint(&mut self, service_name: NamespacedHostname, ep: Endpoint) {
		let ep_uid = ep.workload_uid.clone();
		if let Some(svc) = self.get_by_namespaced_host(&service_name) {
			// We may or may not accept the endpoint based on it's health
			if !svc.should_include_endpoint(ep.status) {
				trace!(
					"service doesn't accept pod with status {:?}, skip",
					ep.status
				);
				return;
			}
			let mut svc = Arc::unwrap_or_clone(svc);

			// Clone the service and add the endpoint.
			svc.endpoints.insert(ep_uid, ep);

			// Update the service.
			self.insert_endpoint_update(svc);
		} else {
			// We received workload endpoints, but don't have the Service yet.
			// This can happen due to ordering issues.
			trace!("pod has service {}, but service not found", service_name);

			// Add a staged entry. This will be added to the service once we receive it.
			self
				.staged_services
				.entry(service_name.clone())
				.or_default()
				.insert(ep_uid, ep.clone());
		}
	}

	/// Removes entries for the given endpoint address.
	fn remove_endpoint(&mut self, prev_workload: &Workload) {
		let mut services_to_update = HashSet::new();
		let workload_uid = &prev_workload.uid;
		for svc in prev_workload.services.iter() {
			// Remove the endpoint from the staged services.
			self
				.staged_services
				.entry(svc.clone())
				.or_default()
				.remove(workload_uid);
			if self.staged_services[svc].is_empty() {
				self.staged_services.remove(svc);
			}

			services_to_update.insert(svc.clone());
		}

		// Now remove the endpoint from all Services.
		for svc in &services_to_update {
			if let Some(svc) = self.get_by_namespaced_host(svc) {
				let mut svc = Arc::unwrap_or_clone(svc);
				svc.endpoints.remove(workload_uid);

				// Update the service.
				self.insert_endpoint_update(svc);
			}
		}
	}

	/// Removes the service for the given host and namespace, and returns whether something was removed
	fn remove(&mut self, namespaced_host: &NamespacedHostname) -> bool {
		match self.by_host.get_mut(&namespaced_host.hostname) {
			None => false,
			Some(services) => {
				// Remove the previous service from the by_host map.
				let Some(prev) = ({
					let mut prev = None;
					for i in 0..services.len() {
						if services[i].namespace == namespaced_host.namespace {
							// Remove this service from the list.
							prev = Some(services.remove(i));

							// If the the services list is empty, remove the entire entry.
							if services.is_empty() {
								self.by_host.remove(&namespaced_host.hostname);
							}
							break;
						}
					}
					prev
				}) else {
					// Not found.
					return false;
				};

				// Remove the entries for the previous service VIPs.
				prev.vips.iter().for_each(|addr| {
					self.by_vip.remove(addr);
				});

				// Remove the staged service.
				// TODO(nmittler): no endpoints for this service should be staged at this point.
				self.staged_services.remove(namespaced_host);

				// Remove successful.
				true
			},
		}
	}

	/// insert_endpoint_update is like insert, but optimized for the case where we know only endpoints change.
	fn insert_endpoint_update(&mut self, service: Service) {
		self.insert_internal(service, true)
	}

	/// Adds the given service.
	fn insert(&mut self, service: Service) {
		self.insert_internal(service, false)
	}

	fn insert_internal(&mut self, mut service: Service, endpoint_update_only: bool) {
		let namespaced_hostname = service.namespaced_hostname();
		// If we're replacing an existing service, remove the old one from all data structures.
		if !endpoint_update_only {
			// First add any staged service endpoints. Due to ordering issues, we may have received
			// the workloads before their associated services.
			if let Some(endpoints) = self.staged_services.remove(&namespaced_hostname) {
				trace!(
					"staged service found, inserting {} endpoints",
					endpoints.len()
				);
				for (wip, ep) in endpoints {
					if service.should_include_endpoint(ep.status) {
						service.endpoints.insert(wip.clone(), ep);
					}
				}
			}

			let _ = self.remove(&namespaced_hostname);
		}

		// Create the Arc.
		let service = Arc::new(service);
		let hostname = &service.hostname;

		// Map the vips to the service.
		for vip in &service.vips {
			self.by_vip.insert(vip.clone(), service.clone());
		}

		// Map the hostname to the service.
		match self.by_host.get_mut(hostname) {
			None => {
				let _ = self.by_host.insert(hostname.clone(), vec![service.clone()]);
			},
			Some(services) => {
				if let Some((cur, _)) = services
					.iter()
					.find_position(|s| s.namespace == service.namespace)
				{
					// Service already exists; replace the slot
					services[cur] = service.clone()
				} else {
					// No service exists yet, append it
					services.push(service.clone());
				}
			},
		}
	}
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

pub struct StoreUpdater {
	state: Arc<RwLock<Store>>,
}

impl StoreUpdater {
	/// Creates a new updater for the given stores.
	pub fn new(state: Arc<RwLock<Store>>) -> Self {
		Self { state }
	}
}

impl Store {
	pub fn insert_address(&mut self, a: XdsAddress) -> anyhow::Result<()> {
		match a.r#type {
			Some(XdsType::Workload(w)) => self.insert_workload(w),
			Some(XdsType::Service(s)) => self.insert_service(s),
			_ => Err(anyhow::anyhow!("unknown address type")),
		}
	}
	#[instrument(
		level = Level::TRACE,
		name="insert_workload",
		skip_all,
		fields(uid=%w.uid),
	)]
	pub fn insert_workload(&mut self, w: XdsWorkload) -> anyhow::Result<()> {
		debug!("handling insert");

		// Clone services, so we can pass full ownership of the rest of XdsWorkload to build our Workload
		// object, which doesn't include Services.
		// In theory, I think we could avoid this if Workload::try_from returning the services.
		// let services = w.services.clone();
		// Convert the workload.
		let (workload, services): (Workload, HashMap<String, PortList>) = w.try_into()?;
		let workload = Arc::new(workload);

		// First, remove the entry entirely to make sure things are cleaned up properly.
		self.remove_workload_for_insert(&workload.uid);

		// Lock and upstate the stores.
		self.workloads.insert(workload.clone());
		self
			.services
			.insert_endpoint_for_services(&workload, &services);

		Ok(())
	}

	#[instrument(
        level = Level::TRACE,
        name="insert_service",
        skip_all,
        fields(name=%service.name),
	)]
	pub fn insert_service(&mut self, service: XdsService) -> anyhow::Result<()> {
		debug!("handling insert");
		let mut service = Service::try_from(&service)?;

		// If the service already exists, add existing endpoints into the new service.
		if let Some(prev) = self
			.services
			.get_by_namespaced_host(&service.namespaced_hostname())
		{
			for ep in prev.endpoints.iter() {
				if service.should_include_endpoint(ep.status) {
					service
						.endpoints
						.insert(ep.workload_uid.clone(), ep.clone());
				}
			}
		}

		self.services.insert(service);
		Ok(())
	}

	fn remove(&mut self, xds_name: &Strng) {
		self.remove_internal(xds_name, false);
	}

	fn remove_workload_for_insert(&mut self, xds_name: &Strng) {
		self.remove_internal(xds_name, true);
	}
	#[instrument(
        level = Level::TRACE,
        name="remove",
        skip_all,
        fields(name=%xds_name, for_workload_insert=%for_workload_insert),
	)]
	fn remove_internal(&mut self, xds_name: &Strng, for_workload_insert: bool) {
		// remove workload by UID; if xds_name is a service then this will no-op
		if let Some(prev) = self.workloads.remove(&strng::new(xds_name)) {
			// Also remove service endpoints for the workload.
			self.services.remove_endpoint(&prev);

			// We removed a workload, no reason to attempt to remove a service with the same name
			return;
		}
		if for_workload_insert {
			// This is a workload, don't attempt to remove as a service
			return;
		}

		let Ok(name) = NamespacedHostname::from_str(xds_name) else {
			// we don't have namespace/hostname xds primary key for service
			warn!("tried to remove service but it did not have the expected namespace/hostname format");
			return;
		};

		if name.hostname.contains('/') {
			// avoid trying to delete obvious workload UIDs as a service,
			// which can result in noisy logs when new workloads are added
			// (we remove then add workloads on initial update)
			//
			// we can make this assumption because namespaces and hostnames cannot have `/` in them
			trace!("not a service, not attempting to delete as such",);
			return;
		}
		if !self.services.remove(&name) {
			warn!("tried to remove service, but it was not found");
		}
	}
}

pub fn network_addr(network: Strng, vip: IpAddr) -> NetworkAddress {
	NetworkAddress {
		network,
		address: vip,
	}
}

impl agent_xds::Handler<XdsAddress> for StoreUpdater {
	fn handle(
		&self,
		updates: Box<&mut dyn Iterator<Item = agent_xds::XdsUpdate<XdsAddress>>>,
	) -> Result<(), Vec<agent_xds::RejectedConfig>> {
		let mut state = self.state.write().unwrap();
		let handle = |res: XdsUpdate<XdsAddress>| {
			match res {
				XdsUpdate::Update(w) => state.insert_address(w.resource)?,
				XdsUpdate::Remove(name) => {
					debug!("handling delete {}", name);
					state.remove(&strng::new(name))
				},
			}
			Ok(())
		};
		agent_xds::handle_single_resource(updates, handle)
	}
}
