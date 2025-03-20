use crate::proto::xds::service::discovery::v3::aggregated_discovery_service_client::AggregatedDiscoveryServiceClient;
use crate::proto::xds::service::discovery::v3::Resource as ProtoResource;
use crate::proto::xds::service::discovery::v3::{DeltaDiscoveryRequest, DeltaDiscoveryResponse, Node, Status};
use prost::{DecodeError, EncodeError};
use prost_types::value::Kind;
use prost_types::{Struct, Value};
use split_iter::Splittable;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::time::Duration;
use std::{fmt, mem};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::{debug, error, info, info_span, warn, Instrument};

#[derive(Eq, Hash, PartialEq, Debug, Clone)]
pub struct ResourceKey {
    pub name: String,
    pub type_url: String,
}

impl Display for ResourceKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.type_url, self.name)
    }
}

#[derive(Debug)]
pub struct RejectedConfig {
    name: String,
    reason: anyhow::Error,
}

impl RejectedConfig {
    pub fn new(name: String, reason: anyhow::Error) -> Self {
        Self { name, reason }
    }
}

impl Display for RejectedConfig {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.reason)
    }
}

struct DisplayStatus<'a>(&'a tonic::Status);

impl<'a> fmt::Display for DisplayStatus<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use std::error::Error;
        let s = &self.0;
        write!(f, "status: {:?}, message: {:?}", s.code(), s.message())?;

        if s.message().to_string().contains("authentication failure") {
            write!(
                f,
                " (hint: check the control plane logs for more information)"
            )?;
        }
        if !s.details().is_empty() {
            if let Ok(st) = std::str::from_utf8(s.details()) {
                write!(f, ", details: {st}")?;
            }
        }
        if let Some(src) = s.source().and_then(|s| s.source()) {
            write!(f, ", source: {src}")?;
            // Error is not public to explicitly match on, so do a fuzzy match
            if format!("{src}").contains("Temporary failure in name resolution") {
                write!(f, " (hint: is the DNS server reachable?)")?;
            }
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("gRPC error {}", DisplayStatus(.0))]
    GrpcStatus(#[from] tonic::Status),
    #[error("gRPC connection error connecting to {0}: {}", DisplayStatus(.1))]
    Connection(String, #[source] tonic::Status),
    #[error("gRPC connection error connecting to {0}: {}", .1)]
    Transport(String, #[source] tonic::transport::Error),
    /// Attempted to send on a MPSC channel which has been canceled
    #[error(transparent)]
    RequestFailure(#[from] Box<mpsc::error::SendError<DeltaDiscoveryRequest>>),
    #[error("failed to send on demand resource")]
    OnDemandSend(),
}

/// handle_single_resource is a helper to process a set of updates with a closure that processes items one-by-one.
/// It handles aggregating errors as NACKS.
pub fn handle_single_resource<T: prost::Message, F: FnMut(XdsUpdate<T>) -> anyhow::Result<()>>(
    updates: impl Iterator<Item = XdsUpdate<T>>,
    mut handle_one: F,
) -> Result<(), Vec<RejectedConfig>> {
    let rejects: Vec<RejectedConfig> = updates
        .filter_map(|res| {
            let name = res.name();
            if let Err(e) = handle_one(res) {
                Some(RejectedConfig::new(name, e))
            } else {
                None
            }
        })
        .collect();
    if rejects.is_empty() {
        Ok(())
    } else {
        Err(rejects)
    }
}

// Handler is responsible for handling a discovery response.
// Handlers can mutate state and return a list of rejected configurations (if there are any).
pub trait Handler<T: prost::Message>: Send + Sync + 'static {
    fn no_on_demand(&self) -> bool {
        false
    }
    fn handle(
        &self,
        res: Box<&mut dyn Iterator<Item = XdsUpdate<T>>>,
    ) -> Result<(), Vec<RejectedConfig>>;
}

// ResponseHandler is responsible for handling a discovery response.
// Handlers can mutate state and return a list of rejected configurations (if there are any).
// This is an internal only trait; public usage uses the Handler type which is typed.
trait RawHandler: Send + Sync + 'static {
    fn handle(
        &self,
        state: &mut State,
        res: DeltaDiscoveryResponse,
    ) -> Result<(), Vec<RejectedConfig>>;
}

// HandlerWrapper is responsible for implementing RawHandler the provided handler.
struct HandlerWrapper<T: prost::Message> {
    h: Box<dyn Handler<T>>,
}

impl<T: 'static + prost::Message + Default> RawHandler for HandlerWrapper<T> {
    fn handle(
        &self,
        state: &mut State,
        res: DeltaDiscoveryResponse,
    ) -> Result<(), Vec<RejectedConfig>> {
        let type_url = res.type_url;
        let removes = &res.removed_resources;

        // Keep track of any failures but keep going
        let (decode_failures, updates) = res
            .resources
            .iter()
            .map(|raw| {
                decode_proto::<T>(raw).map_err(|err| RejectedConfig {
                    name: raw.name.as_str().into(),
                    reason: err.into(),
                })
            })
            .split(|i| i.is_ok());

        let mut updates = updates
            // We already filtered to ok
            .map(|r| r.expect("must be ok"))
            .map(XdsUpdate::Update)
            .chain(removes.iter().cloned().map(|s| XdsUpdate::Remove(s)));

        // First, call handlers that update the proxy state.
        // other wise on-demand notifications might observe a cache without their resource
        let updates: Box<&mut dyn Iterator<Item = XdsUpdate<T>>> = Box::new(&mut updates);
        let result = self.h.handle(updates);

        // Collecting after handle() is important, as the split() will cache the side we use last.
        // Updates >>> Errors (hopefully), so we want this one to do the allocations.
        let decode_failures: Vec<_> = decode_failures
            .map(|r| r.expect_err("must be err"))
            .collect();

        // after we update the proxy cache, we can update our xds cache. it's important that we do this after
        // as we make on demand notifications here, so the proxy cache must be updated first.
        for name in res.removed_resources {
            let k = ResourceKey {
                name,
                type_url: type_url.clone(),
            };
            debug!("received delete resource {k}");
            if let Some(rm) = state.known_resources.get_mut(&k.type_url) {
                rm.remove(&k.name);
            }
            state.notify_on_demand(&k);
        }

        for r in res.resources {
            let key = ResourceKey {
                name: r.name,
                type_url: type_url.clone(),
            };
            state.notify_on_demand(&key);
            state.add_resource(key.type_url, key.name);
        }

        // Either can fail. Merge the results
        match (result, decode_failures.is_empty()) {
            (Ok(()), true) => Ok(()),
            (Ok(_), false) => Err(decode_failures),
            (r @ Err(_), true) => r,
            (Err(mut rejects), false) => {
                rejects.extend(decode_failures);
                Err(rejects)
            }
        }
    }
}

pub struct Config {
    address: String,
    proxy_metadata: HashMap<String, String>,
    handlers: HashMap<String, Box<dyn RawHandler>>,
    initial_requests: Vec<DeltaDiscoveryRequest>,
    on_demand: bool,
}

pub struct State {
    /// Stores all known workload resources. Map from type_url to name
    known_resources: HashMap<String, HashSet<String>>,

    /// pending stores a list of all resources that are pending and XDS push
    pending: HashMap<ResourceKey, oneshot::Sender<()>>,

    demand: mpsc::Receiver<(oneshot::Sender<()>, ResourceKey)>,
    demand_tx: mpsc::Sender<(oneshot::Sender<()>, ResourceKey)>,
}

impl State {
    fn notify_on_demand(&mut self, key: &ResourceKey) {
        if let Some(send) = self.pending.remove(key) {
            debug!("on demand notify {}", key.name);
            if send.send(()).is_err() {
                warn!("on demand dropped event for {}", key.name)
            }
        }
    }
    fn add_resource(&mut self, type_url: String, name: String) {
        self.known_resources
            .entry(type_url)
            .or_default()
            .insert(name.clone());
    }
}

impl Config {
    pub fn new() -> Config {
        Config {
            address: "localhost:15010".to_string(),
            handlers: HashMap::new(),
            initial_requests: Vec::new(),
            on_demand: false,
            proxy_metadata: HashMap::new(),
        }
    }

    pub fn with_watched_handler<F>(self, type_url: String, f: impl Handler<F>) -> Config
    where
        F: 'static + prost::Message + Default,
    {
        let no_on_demand = f.no_on_demand();
        self.with_handler(type_url.clone(), f)
            .watch(type_url, no_on_demand)
    }

    fn with_handler<F>(mut self, type_url: String, f: impl Handler<F>) -> Config
    where
        F: 'static + prost::Message + Default,
    {
        let h = HandlerWrapper { h: Box::new(f) };
        self.handlers.insert(type_url, Box::new(h));
        self
    }

    fn watch(mut self, type_url: String, no_on_demand: bool) -> Config {
        self.initial_requests
            .push(self.construct_initial_request(type_url, no_on_demand));
        self
    }

    fn build_struct<T: IntoIterator<Item = (S, S)>, S: ToString>(a: T) -> Struct {
        let fields = BTreeMap::from_iter(a.into_iter().map(|(k, v)| {
            (
                k.to_string(),
                Value {
                    kind: Some(Kind::StringValue(v.to_string())),
                },
            )
        }));
        Struct { fields }
    }

    fn node(&self) -> Node {
        let meta = Self::build_struct(self.proxy_metadata.clone());

        Node {
            id: "test".to_string(),
            metadata: Some(meta),
            ..Default::default()
        }
    }
    fn construct_initial_request(
        &self,
        request_type: String,
        no_on_demand: bool,
    ) -> DeltaDiscoveryRequest {
        let node = self.node();

        let (sub, unsub) = if (!no_on_demand) && self.on_demand {
            // XDS doesn't have a way to subscribe to zero resources. We workaround this by subscribing and unsubscribing
            // in one event, effectively giving us "subscribe to nothing".
            (vec!["*".to_string()], vec!["*".to_string()])
        } else {
            (vec![], vec![])
        };
        DeltaDiscoveryRequest {
            type_url: request_type.to_string(),
            node: Some(node.clone()),
            resource_names_subscribe: sub,
            resource_names_unsubscribe: unsub,
            ..Default::default()
        }
    }

    pub fn build(self, block_ready: tokio::sync::watch::Sender<()>) -> AdsClient {
        AdsClient::new(self, block_ready)
    }
}

/// AdsClient provides a (mostly) generic DeltaAggregatedResources XDS client.
///
/// The client works by accepting arbitrary handlers for types, configured by user.
/// These handlers can do whatever they want with incoming responses, but are responsible for maintaining their own state.
/// For example, if a usage wants to keep track of all Foo resources received, it needs to handle the add/removes in the configured handler.
///
/// The client also supports on-demand lookup of resources; see demander() for more information.
///
/// Currently, this is not quite a fully general purpose XDS client, as there is no dependant resource support.
/// This could be added if needed, though.
pub struct AdsClient {
    config: Config,

    state: State,

    block_ready: Option<tokio::sync::watch::Sender<()>>,

    connection_id: u32,
    types_to_expect: HashSet<String>,
}

/// Demanded allows awaiting for an on-demand XDS resource
pub struct Demanded {
    b: oneshot::Receiver<()>,
}

impl Demanded {
    /// recv awaits for the requested resource
    /// Note: the actual resource is not directly returned. Instead, callers are notified that the event
    /// has been handled through the configured resource handler.
    pub async fn recv(self) {
        let _ = self.b.await;
    }
}

/// Demander allows requesting XDS resources on-demand
#[derive(Debug, Clone)]
pub struct Demander {
    demand: mpsc::Sender<(oneshot::Sender<()>, ResourceKey)>,
}

#[derive(Debug)]
enum XdsSignal {
    None,
    Ack,
    Nack,
}

impl Display for XdsSignal {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            XdsSignal::None => "NONE",
            XdsSignal::Ack => "ACK",
            XdsSignal::Nack => "NACK",
        })
    }
}

impl Demander {
    /// Demand requests a given workload by name
    pub async fn demand(&self, type_url: String, name: String) -> Demanded {
        let (tx, rx) = oneshot::channel::<()>();
        self.demand
            .send((tx, ResourceKey { name, type_url }))
            .await
            // TODO: is this guaranteed? How can we handle the failure
            .expect("demand channel should not close");
        Demanded { b: rx }
    }
}

const INITIAL_BACKOFF: Duration = Duration::from_millis(10);
const MAX_BACKOFF: Duration = Duration::from_secs(15);

impl AdsClient {
    fn is_initial_request_on_demand(r: &DeltaDiscoveryRequest) -> bool {
        !r.resource_names_subscribe.is_empty()
    }

    fn new(config: Config, block_ready: tokio::sync::watch::Sender<()>) -> Self {
        let (tx, rx) = mpsc::channel(100);
        let state = State {
            known_resources: Default::default(),
            pending: Default::default(),
            demand: rx,
            demand_tx: tx,
        };
        let types_to_expect: HashSet<String> = config
            .initial_requests
            .iter()
            .filter(|e| !Self::is_initial_request_on_demand(e)) // is_empty implies not ondemand
            .map(|e| e.type_url.clone())
            .collect();
        AdsClient {
            config,
            state,
            block_ready: Some(block_ready),
            connection_id: 0,
            types_to_expect,
        }
    }

    /// demander returns a Demander instance which can be used to request resources on-demand
    pub fn demander(&self) -> Option<Demander> {
        if self.config.on_demand {
            Some(Demander {
                demand: self.state.demand_tx.clone(),
            })
        } else {
            None
        }
    }

    async fn run_loop(&mut self, backoff: Duration) -> Duration {
        match self.run_internal().await {
            Err(e @ Error::Connection(_, _)) => {
                // For connection errors, we add backoff
                let backoff = std::cmp::min(MAX_BACKOFF, backoff * 2);
                warn!(
                    "XDS client connection error: {}, retrying in {:?}",
                    e, backoff
                );
                tokio::time::sleep(backoff).await;
                backoff
            }
            Err(ref e @ Error::GrpcStatus(ref status)) => {
                let err_detail = e.to_string();
                let backoff = if status.code() == tonic::Code::Unknown
                    || status.code() == tonic::Code::Cancelled
                    || status.code() == tonic::Code::DeadlineExceeded
                    || (status.code() == tonic::Code::Unavailable
                        && status.message().contains("transport is closing"))
                    || (status.code() == tonic::Code::Unavailable
                        && status.message().contains("received prior goaway"))
                {
                    debug!(
                        "XDS client terminated: {}, retrying in {:?}",
                        err_detail, backoff
                    );
                    INITIAL_BACKOFF
                } else {
                    warn!(
                        "XDS client error: {}, retrying in {:?}",
                        err_detail, backoff
                    );
                    // For gRPC errors, we add backoff
                    std::cmp::min(MAX_BACKOFF, backoff * 2)
                };
                tokio::time::sleep(backoff).await;
                backoff
            }
            Err(e) => {
                // For other errors, we connect immediately
                // TODO: we may need more nuance here; if we fail due to invalid initial request we may overload
                // But we want to reconnect from MaxConnectionAge immediately.
                warn!("XDS client error: {}, retrying", e);
                // Reset backoff
                INITIAL_BACKOFF
            }
            Ok(_) => {
                warn!("XDS client complete");
                // Reset backoff
                INITIAL_BACKOFF
            }
        }
    }

    pub async fn run(mut self) -> Result<(), Error> {
        let mut backoff = INITIAL_BACKOFF;
        loop {
            self.connection_id += 1;
            let id = self.connection_id;
            backoff = self
                .run_loop(backoff)
                .instrument(info_span!("xds", id))
                .await;
        }
    }

    async fn run_internal(&mut self) -> Result<(), Error> {
        let (discovery_req_tx, mut discovery_req_rx) = mpsc::channel::<DeltaDiscoveryRequest>(100);
        // For each type in initial_watches we will send a request on connection to subscribe
        let initial_requests: Vec<DeltaDiscoveryRequest> = self
            .config
            .initial_requests
            .iter()
            .map(|e| {
                let mut req = e.clone();
                req.initial_resource_versions = self
                    .state
                    .known_resources
                    .get(&req.type_url)
                    .map(|hs| {
                        hs.iter()
                            .map(|n| (n.to_string(), "".to_string())) // Proto expects Name -> Version. We don't care about version
                            .collect()
                    })
                    .unwrap_or_default();
                req
            })
            .collect();

        let outbound = async_stream::stream! {
            for initial in initial_requests {
                debug!(resources=initial.initial_resource_versions.len(), type_url=initial.type_url, "sending initial request");
                yield initial;
            }
            while let Some(message) = discovery_req_rx.recv().await {
                debug!(type_url=message.type_url, "sending request");
                yield message
            }
            warn!("outbound stream complete");
        };

        let addr = self.config.address.clone();

        let ads_connection = AggregatedDiscoveryServiceClient::connect(addr.clone())
            .await
            .map_err(|src| Error::Transport(addr.clone(), src))?
            .max_decoding_message_size(200 * 1024 * 1024)
            .delta_aggregated_resources(tonic::Request::new(outbound))
            .await;

        let mut response_stream = ads_connection
            .map_err(|src| Error::Connection(addr, src))?
            .into_inner();
        debug!("connected established");

        info!("Stream established");
        loop {
            tokio::select! {
                _demand_event = self.state.demand.recv() => {
                    self.handle_demand_event(_demand_event, &discovery_req_tx).await?;
                }
                msg = response_stream.message() => {
                    let msg = msg?;
                    let mut received_type = None;
                    if !self.types_to_expect.is_empty() {
                        received_type = msg.as_ref().map(|e| e.type_url.clone());
                    }
                    if let XdsSignal::Ack = self.handle_stream_event(msg, &discovery_req_tx).await? {
                        if let Some(received_type) = received_type {
                            self.types_to_expect.remove(&received_type);
                            if self.types_to_expect.is_empty() {
                                mem::drop(mem::take(&mut self.block_ready));
                            }
                        }
                    };
                }
            }
        }
    }

    async fn handle_stream_event(
        &mut self,
        stream_event: Option<DeltaDiscoveryResponse>,
        send: &mpsc::Sender<DeltaDiscoveryRequest>,
    ) -> Result<XdsSignal, Error> {
        let Some(response) = stream_event else {
            return Ok(XdsSignal::None);
        };
        let type_url = response.type_url.clone();
        let nonce = response.nonce.clone();
        info!(
            type_url = type_url, // this is a borrow, it's OK
            size = response.resources.len(),
            removes = response.removed_resources.len(),
            "received response"
        );
        let handler_response: Result<(), Vec<RejectedConfig>> =
            match self.config.handlers.get(&type_url) {
                Some(h) => h.handle(&mut self.state, response),
                None => {
                    error!(%type_url, "unknown type");
                    // TODO: this will just send another discovery request, to server. We should
                    // either send one with an error or not send one at all.
                    Ok(())
                }
            };

        let (response_type, error) = match handler_response {
            Err(rejects) => {
                let error = rejects
                    .into_iter()
                    .map(|reject| reject.to_string())
                    .collect::<Vec<String>>()
                    .join("; ");
                (XdsSignal::Nack, Some(error))
            }
            _ => (XdsSignal::Ack, None),
        };

        match response_type {
            XdsSignal::Nack => error!(
                type_url=type_url,
                nonce,
                "type"=?response_type,
                error=error,
                "sending response",
            ),
            _ => debug!(
                type_url=type_url,
                nonce,
                "type"=?response_type,
                "sending response",
            ),
        };

        send.send(DeltaDiscoveryRequest {
            type_url,              // this is owned, OK to move
            response_nonce: nonce, // this is owned, OK to move
            error_detail: error.map(|msg| Status {
                message: msg,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .map_err(|e| Error::RequestFailure(Box::new(e)))
        .map(|_| response_type)
    }

    async fn handle_demand_event(
        &mut self,
        demand_event: Option<(oneshot::Sender<()>, ResourceKey)>,
        send: &mpsc::Sender<DeltaDiscoveryRequest>,
    ) -> Result<(), Error> {
        let Some((tx, demand_event)) = demand_event else {
            return Ok(());
        };
        info!("received on demand request {demand_event}");
        let ResourceKey { type_url, name } = demand_event.clone();
        self.state.pending.insert(demand_event, tx);
        self.state.add_resource(type_url.clone(), name.clone());
        send.send(DeltaDiscoveryRequest {
            type_url: type_url.to_string(),
            resource_names_subscribe: vec![name.to_string()],
            ..Default::default()
        })
        .await
        .map_err(|e| Error::RequestFailure(Box::new(e)))?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct XdsResource<T: prost::Message> {
    pub name: String,
    pub resource: T,
}

#[derive(Debug)]
pub enum XdsUpdate<T: prost::Message> {
    Update(XdsResource<T>),
    Remove(String),
}

impl<T: prost::Message> XdsUpdate<T> {
    pub fn name(&self) -> String {
        match self {
            XdsUpdate::Update(ref r) => r.name.clone(),
            XdsUpdate::Remove(n) => n.clone(),
        }
    }
}

fn decode_proto<T: prost::Message + Default>(
    resource: &ProtoResource,
) -> Result<XdsResource<T>, AdsError> {
    let name = resource.name.as_str().into();
    resource
        .resource
        .as_ref()
        .ok_or(AdsError::MissingResource())
        .and_then(|res| <T>::decode(&res.value[..]).map_err(AdsError::Decode))
        .map(|r| XdsResource { name, resource: r })
}

#[derive(Clone, Debug, Error)]
pub enum AdsError {
    #[error("unknown resource type: {0}")]
    UnknownResourceType(String),
    #[error("decode: {0}")]
    Decode(#[from] DecodeError),
    #[error("XDS payload without resource")]
    MissingResource(),
    #[error("encode: {0}")]
    Encode(#[from] EncodeError),
}
