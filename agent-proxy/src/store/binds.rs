use crate::store::Event;
use crate::types::agent::{Bind, BindName, Listener, ListenerName, ListenerSet, Route, RouteName};
use crate::types::proto::adp::Resource as ADPResource;
use crate::*;
use agent_xds::{RejectedConfig, XdsUpdate};
use futures_core::Stream;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tracing::Level;
use tracing::instrument;

use crate::types::proto::adp::Bind as XdsBind;
use crate::types::proto::adp::Listener as XdsListener;
use crate::types::proto::adp::Route as XdsRoute;
use crate::types::proto::adp::resource::Kind as XdsKind;

#[derive(Debug)]
pub struct Store {
	/// Allows for lookup of services by network address, the service's xds secondary key.
	pub(super) by_name: HashMap<BindName, Arc<Bind>>,
	tx: tokio::sync::broadcast::Sender<Event<Arc<Bind>>>,
}

impl Default for Store {
	fn default() -> Self {
		Self::new()
	}
}
impl Store {
	pub fn new() -> Self {
		let (tx, _) = tokio::sync::broadcast::channel(10);
		Self {
			by_name: Default::default(),
			tx,
		}
	}
	pub fn subscribe(
		&self,
	) -> (impl Stream<Item = Result<Event<Arc<Bind>>, BroadcastStreamRecvError>> + use<>) {
		let sub = self.tx.subscribe();
		tokio_stream::wrappers::BroadcastStream::new(sub)
	}

	pub fn listeners(&self, bind: BindName) -> Option<ListenerSet> {
		// TODO: clone here is terrible!!!
		self.by_name.get(&bind).map(|b| b.listeners.clone())
	}

	pub fn all(&self) -> Vec<Arc<Bind>> {
		self.by_name.values().cloned().collect()
	}

	#[instrument(
        level = Level::INFO,
        name="remove_bind",
        skip_all,
        fields(bind),
    )]
	pub fn remove_bind(&mut self, bind: BindName) {
		if let Some(old) = self.by_name.remove(&bind) {
			let _ = self.tx.send(Event::Remove(old));
		}
	}

	#[instrument(
        level = Level::INFO,
        name="remove_listener",
        skip_all,
        fields(listener),
    )]
	pub fn remove_listener(&mut self, listener: ListenerName) {
		let Some(bind) = self
			.by_name
			.values()
			.find(|v| v.listeners.contains(&listener))
		else {
			return;
		};
		let mut bind = Arc::unwrap_or_clone(bind.clone());
		bind.listeners.remove(&listener);
		self.insert_bind(bind);
	}

	#[instrument(
        level = Level::INFO,
        name="remove_route",
        skip_all,
        fields(route),
    )]
	pub fn remove_route(&mut self, route: RouteName) {
		let Some((_, bind, listener)) = self.by_name.iter().find_map(|(k, v)| {
			let l = v.listeners.iter().find(|l| l.routes.contains(&route));
			l.map(|l| (k.clone(), v.clone(), l.clone()))
		}) else {
			return;
		};
		let mut bind = Arc::unwrap_or_clone(bind.clone());
		let ln = listener.name.clone();
		let mut lis = listener.clone();
		lis.routes.remove(&route);
		bind.listeners.insert(ln, lis);
		self.insert_bind(bind);
	}

	#[instrument(
        level = Level::INFO,
        name="insert_bind",
        skip_all,
        fields(bind=%bind.name),
    )]
	pub fn insert_bind(&mut self, bind: Bind) {
		// TODO: handle update
		let arc = Arc::new(bind);
		self.by_name.insert(arc.name.clone(), arc.clone());
		// ok to have no subs
		let _ = self.tx.send(Event::Add(arc));
	}
	#[instrument(
        level = Level::INFO,
        name="insert_listener",
        skip_all,
        fields(listener=%lis.name,bind=%bind_name),
    )]
	pub fn insert_listener(&mut self, lis: Listener, bind_name: BindName) {
		if let Some(b) = self.by_name.get(&bind_name) {
			let mut bind = Arc::unwrap_or_clone(b.clone());
			bind.listeners.insert(lis.name.clone(), lis);
			self.insert_bind(bind);
		} else {
			warn!("no bind found");
		}
	}
	#[instrument(
        level = Level::INFO,
        name="insert_route",
        skip_all,
        fields(listener=%ln,route=%r.name),
    )]
	pub fn insert_route(&mut self, r: Route, ln: ListenerName) {
		let Some((bind, lis)) = self
			.by_name
			.values()
			.find_map(|l| l.listeners.get(&ln).map(|ls| (l, ls)))
		else {
			warn!("no listener found");
			return;
		};
		let mut bind = Arc::unwrap_or_clone(bind.clone());
		let mut lis = lis.clone();
		lis.routes.insert(r.name.clone(), r);
		bind.listeners.insert(ln, lis);
		self.insert_bind(bind);
	}

	fn remove_resource(&mut self, res: &Strng) {
		trace!("removing res {res}...");
		let Some((res, res_name)) = res.split_once("/") else {
			trace!("unknown resource name {res}");
			return;
		};
		match res {
			"bind" => {
				self.remove_bind(strng::new(res_name));
			},
			"listener" => {
				self.remove_listener(strng::new(res_name));
			},
			"route" => {
				self.remove_route(strng::new(res_name));
			},
			_ => {
				error!("unknown resource kind {res}");
			},
		}
	}

	fn insert_xds(&mut self, res: ADPResource) -> anyhow::Result<()> {
		trace!("insert resource {res:?}");
		match res.kind {
			Some(XdsKind::Bind(w)) => self.insert_xds_bind(w),
			Some(XdsKind::Listener(w)) => self.insert_xds_listener(w),
			Some(XdsKind::Route(w)) => self.insert_xds_route(w),
			_ => Err(anyhow::anyhow!("unknown resource type")),
		}
	}

	fn insert_xds_bind(&mut self, raw: XdsBind) -> anyhow::Result<()> {
		let bind = Bind::try_from(&raw)?;
		self.insert_bind(bind);
		Ok(())
	}
	fn insert_xds_listener(&mut self, raw: XdsListener) -> anyhow::Result<()> {
		let (lis, bind_name): (Listener, BindName) = (&raw).try_into()?;
		self.insert_listener(lis, bind_name);
		Ok(())
	}
	fn insert_xds_route(&mut self, raw: XdsRoute) -> anyhow::Result<()> {
		let (route, listener_name): (Route, ListenerName) = (&raw).try_into()?;
		self.insert_route(route, listener_name);
		Ok(())
	}
}

pub struct StoreUpdater {
	state: Arc<RwLock<Store>>,
}

impl agent_xds::Handler<ADPResource> for StoreUpdater {
	fn handle(
		&self,
		updates: Box<&mut dyn Iterator<Item = XdsUpdate<ADPResource>>>,
	) -> Result<(), Vec<RejectedConfig>> {
		let mut state = self.state.write().unwrap();
		let handle = |res: XdsUpdate<ADPResource>| {
			match res {
				XdsUpdate::Update(w) => state.insert_xds(w.resource)?,
				XdsUpdate::Remove(name) => {
					debug!("handling delete {}", name);
					state.remove_resource(&strng::new(name))
				},
			}
			Ok(())
		};
		agent_xds::handle_single_resource(updates, handle)
	}
}
