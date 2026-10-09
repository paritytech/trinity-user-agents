//! Native `ChainProvider`s: WebSocket JSON-RPC to the preset's public nodes, or
//! the embedded smoldot light client.
//!
//! The headless hosts reach the real People-chain statement store. Over RPC
//! every `connect` opens a fresh socket; the runtime's `HostRpcClient` sits on
//! top and speaks statement-store RPC.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc};
use tokio_stream::wrappers::BroadcastStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, warn};
use truapi::latest as api;
use truapi::platform::{ChainProvider, HopProvider, JsonRpcConnection, ProviderError};
use truapi_provider::EmbeddedChainProvider;

use crate::network::{ChainEndpoint, HopEndpoint, NetworkConfig};

/// Broadcast backlog for inbound JSON-RPC frames per connection.
const INBOUND_CHANNEL_CAPACITY: usize = 1024;
const MAX_HOP_CONNECTIONS: usize = 8;

/// Chain provider that maps a requested genesis hash to a WebSocket endpoint.
///
/// The all-zero genesis (the headless SSO sentinel) and any unmapped genesis
/// fall back to the People-chain statement store. Every role the preset serves —
/// People, Bulletin and Asset Hub — is always routed; the test switch only widens
/// routing to endpoints the preset carries without serving them as a role.
pub struct WsChainProvider {
    fallback_url: String,
    by_genesis: HashMap<[u8; 32], String>,
    hop_by_genesis: HashMap<[u8; 32], Vec<String>>,
    hop_connections: Arc<Semaphore>,
}

impl WsChainProvider {
    pub fn new(fallback_url: impl Into<String>, live_chain_endpoints: &[ChainEndpoint]) -> Self {
        let live_chain_routing = std::env::var("E2E_LIVE_CHAIN").as_deref() == Ok("1");
        Self::with_live_chain_routing(fallback_url, live_chain_endpoints, live_chain_routing)
    }

    fn with_live_chain_routing(
        fallback_url: impl Into<String>,
        live_chain_endpoints: &[ChainEndpoint],
        live_chain_routing: bool,
    ) -> Self {
        // People remains the fallback for the SSO sentinel. Bulletin backs preimage
        // submission and Asset Hub backs PGAS claims, so all three are host
        // dependencies and must never be gated by the product-facing Chain/* switch.
        let by_genesis = live_chain_endpoints
            .iter()
            .filter(|endpoint| endpoint.required_for_host || live_chain_routing)
            .map(|endpoint| (endpoint.genesis, endpoint.ws.to_string()))
            .collect();
        Self {
            fallback_url: fallback_url.into(),
            by_genesis,
            hop_by_genesis: HashMap::new(),
            hop_connections: Arc::new(Semaphore::new(MAX_HOP_CONNECTIONS)),
        }
    }

    pub fn with_hop_endpoints(
        mut self,
        bulletin_genesis: [u8; 32],
        endpoints: &[HopEndpoint],
    ) -> Self {
        self.hop_by_genesis.clear();
        for endpoint in endpoints {
            // A URL trusted for another Bulletin is not trusted for this one.
            if endpoint.bulletin_genesis == bulletin_genesis {
                self.hop_by_genesis
                    .entry(bulletin_genesis)
                    .or_default()
                    .push(endpoint.ws.to_string());
            }
        }
        self
    }

    /// Whether a genesis is mapped rather than answered by the fallback.
    ///
    /// Test-only because production has no reason to care: `url_for` resolves either
    /// way. A test does, since the fallback is the People URL, so asserting on the
    /// resolved URL cannot tell a routed People from a dropped one.
    #[cfg(test)]
    fn routes(&self, genesis_hash: &[u8; 32]) -> bool {
        self.by_genesis.contains_key(genesis_hash)
    }

    /// The URL a genesis routes to. Test-only, so a routing override can be
    /// asserted rather than assumed: an override written to the wrong field
    /// compiles, changes nothing, and is invisible in timings.
    #[cfg(test)]
    pub fn routed_url(&self, genesis_hash: &[u8; 32]) -> &str {
        self.url_for(genesis_hash)
    }

    fn url_for(&self, genesis_hash: &[u8; 32]) -> &str {
        self.by_genesis
            .get(genesis_hash)
            .map(String::as_str)
            .unwrap_or(&self.fallback_url)
    }
}

#[async_trait]
impl ChainProvider for WsChainProvider {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
        let url = self.url_for(&genesis_hash);
        debug!(genesis = %hex::encode(genesis_hash), %url, "chain connect");
        let connection = WsJsonRpcConnection::connect(url, None)
            .await
            .map_err(|reason| ProviderError::Transport { reason })?;
        Ok(Box::new(connection))
    }
}

#[async_trait]
impl HopProvider for WsChainProvider {
    async fn allowed_hop_endpoints(
        &self,
        bulletin_genesis_hash: [u8; 32],
    ) -> Result<Vec<String>, api::GenericError> {
        Ok(self
            .hop_by_genesis
            .get(&bulletin_genesis_hash)
            .cloned()
            .unwrap_or_default())
    }

    async fn connect_hop(
        &self,
        bulletin_genesis_hash: [u8; 32],
        endpoint: String,
    ) -> Result<Box<dyn JsonRpcConnection>, api::GenericError> {
        let allowed = self
            .hop_by_genesis
            .get(&bulletin_genesis_hash)
            .ok_or_else(|| api::GenericError {
                reason: "HOP provider unavailable for this Bulletin chain".to_string(),
            })?;
        truapi::platform::ensure_allowed_hop_endpoint(&endpoint, allowed)?;
        let permit = self
            .hop_connections
            .clone()
            .try_acquire_owned()
            .map_err(|_| api::GenericError {
                reason: "HOP connection limit reached".to_string(),
            })?;
        let connection = WsJsonRpcConnection::connect(&endpoint, Some(permit))
            .await
            .map_err(|reason| api::GenericError { reason })?;
        Ok(Box::new(connection))
    }
}

/// How the headless hosts reach the chains.
///
/// RPC dials the preset's public nodes. `TRUAPI_LIGHT_CLIENT=1` runs the embedded
/// light client instead, which needs no node, so a public node rate-limiting a
/// shared address (a CI runner) cannot refuse the host.
pub enum CliChainProvider {
    /// The preset's public nodes, the default.
    Rpc(WsChainProvider),
    /// Resolves every chain of the preset from the provider's bundled network
    /// catalog by genesis hash.
    LightClient {
        provider: Box<EmbeddedChainProvider>,
        people_genesis: [u8; 32],
        /// HOP endpoints are private WSS relays, not chain nodes, so they stay on
        /// WebSocket.
        hop: WsChainProvider,
        /// One idle connection per chain for the life of the process. The light
        /// client stops a chain when its last connection closes and the runtime
        /// connects once per call, so without it every call would sync again and
        /// start with no peers to broadcast a statement to.
        kept_open: tokio::sync::Mutex<HashMap<[u8; 32], Box<dyn JsonRpcConnection>>>,
    },
}

impl CliChainProvider {
    /// Light client when `TRUAPI_LIGHT_CLIENT=1`, RPC otherwise.
    pub fn new(network: NetworkConfig) -> Self {
        let ws = WsChainProvider::new(network.people_ws, network.live_chain_endpoints)
            .with_hop_endpoints(network.bulletin_genesis, network.hop_endpoints);
        if std::env::var("TRUAPI_LIGHT_CLIENT").as_deref() == Ok("1") {
            Self::LightClient {
                provider: Box::new(EmbeddedChainProvider::builder().build()),
                people_genesis: network.people_genesis,
                hop: ws,
                kept_open: tokio::sync::Mutex::default(),
            }
        } else {
            Self::Rpc(ws)
        }
    }
}

#[async_trait]
impl ChainProvider for CliChainProvider {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
        match self {
            Self::Rpc(provider) => provider.connect(genesis_hash).await,
            Self::LightClient {
                provider,
                people_genesis,
                kept_open,
                ..
            } => {
                // The all-zero SSO sentinel is answered by People, as over RPC.
                let genesis = if genesis_hash == [0; 32] {
                    *people_genesis
                } else {
                    genesis_hash
                };
                debug!(genesis = %hex::encode(genesis), "light client connect");
                if let Entry::Vacant(slot) = kept_open.lock().await.entry(genesis) {
                    slot.insert(provider.connect(genesis).await?);
                }
                provider.connect(genesis).await
            }
        }
    }
}

#[async_trait]
impl HopProvider for CliChainProvider {
    async fn allowed_hop_endpoints(
        &self,
        bulletin_genesis_hash: [u8; 32],
    ) -> Result<Vec<String>, api::GenericError> {
        let (Self::Rpc(ws) | Self::LightClient { hop: ws, .. }) = self;
        ws.allowed_hop_endpoints(bulletin_genesis_hash).await
    }

    async fn connect_hop(
        &self,
        bulletin_genesis_hash: [u8; 32],
        endpoint: String,
    ) -> Result<Box<dyn JsonRpcConnection>, api::GenericError> {
        let (Self::Rpc(ws) | Self::LightClient { hop: ws, .. }) = self;
        ws.connect_hop(bulletin_genesis_hash, endpoint).await
    }
}

/// One WebSocket JSON-RPC connection: outbound requests are queued to a writer
/// task, inbound frames are broadcast to every `responses()` stream.
pub struct WsJsonRpcConnection {
    outbound: mpsc::UnboundedSender<Message>,
    state: Arc<WsConnectionState>,
    /// Receiver created before the reader task starts. The first response
    /// stream takes it so an immediate RPC response cannot race subscription
    /// setup and disappear while the broadcast channel has no receivers.
    initial_inbound: Mutex<Option<broadcast::Receiver<String>>>,
}

struct WsConnectionState {
    inbound: Mutex<Option<broadcast::Sender<String>>>,
    closed: AtomicBool,
    tasks: Mutex<Vec<tokio::task::AbortHandle>>,
    hop_permit: Mutex<Option<OwnedSemaphorePermit>>,
}

impl WsConnectionState {
    fn register_task(&self, task: tokio::task::AbortHandle) {
        let mut tasks = self.tasks.lock().expect("websocket task mutex poisoned");
        if self.closed.load(Ordering::Acquire) {
            task.abort();
        } else {
            tasks.push(task);
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        for task in self
            .tasks
            .lock()
            .expect("websocket task mutex poisoned")
            .drain(..)
        {
            task.abort();
        }
        self.inbound
            .lock()
            .expect("websocket inbound mutex poisoned")
            .take();
        self.hop_permit
            .lock()
            .expect("HOP lease mutex poisoned")
            .take();
    }
}

impl WsJsonRpcConnection {
    async fn connect(url: &str, hop_permit: Option<OwnedSemaphorePermit>) -> Result<Self, String> {
        let (stream, _response) = connect_async(url)
            .await
            .map_err(|err| format!("JSON-RPC websocket connect failed: {err}"))?;
        let (mut write, mut read) = stream.split();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Message>();
        let (inbound, initial_inbound) = broadcast::channel(INBOUND_CHANNEL_CAPACITY);
        let state = Arc::new(WsConnectionState {
            inbound: Mutex::new(Some(inbound)),
            closed: AtomicBool::new(false),
            tasks: Mutex::new(Vec::with_capacity(2)),
            hop_permit: Mutex::new(hop_permit),
        });

        let writer_state = state.clone();
        let writer = tokio::spawn(async move {
            while let Some(message) = outbound_rx.recv().await {
                if write.send(message).await.is_err() {
                    break;
                }
            }
            writer_state.close();
        });
        state.register_task(writer.abort_handle());

        let reader_state = state.clone();
        let reader = tokio::spawn(async move {
            while let Some(message) = read.next().await {
                let response = match message {
                    Ok(Message::Text(text)) => Some(text.to_string()),
                    Ok(Message::Binary(bytes)) => String::from_utf8(bytes.to_vec()).ok(),
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(_) => None,
                };
                if let Some(response) = response
                    && let Some(inbound) = reader_state
                        .inbound
                        .lock()
                        .expect("websocket inbound mutex poisoned")
                        .as_ref()
                {
                    let _ = inbound.send(response);
                }
            }
            reader_state.close();
        });
        state.register_task(reader.abort_handle());

        Ok(Self {
            outbound: outbound_tx,
            state,
            initial_inbound: Mutex::new(Some(initial_inbound)),
        })
    }
}

impl JsonRpcConnection for WsJsonRpcConnection {
    fn send(&self, request: String) {
        if self.state.closed.load(Ordering::Acquire) {
            return;
        }
        let _ = self.outbound.send(Message::Text(request));
    }

    fn responses(&self) -> BoxStream<'static, String> {
        let receiver = self
            .initial_inbound
            .lock()
            .expect("initial chain response receiver mutex poisoned")
            .take()
            .or_else(|| {
                self.state
                    .inbound
                    .lock()
                    .expect("websocket inbound mutex poisoned")
                    .as_ref()
                    .map(broadcast::Sender::subscribe)
            });
        let Some(receiver) = receiver else {
            return futures::stream::empty().boxed();
        };
        BroadcastStream::new(receiver)
            .filter_map(|item| async move {
                match item {
                    Ok(response) => Some(response),
                    Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(
                        dropped,
                    )) => {
                        warn!(dropped, "chain response subscriber lagged");
                        None
                    }
                }
            })
            .boxed()
    }

    fn close(&self) {
        self.state.close();
    }
}

impl Drop for WsJsonRpcConnection {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use clap::ValueEnum;
    use futures::FutureExt;

    use super::*;
    use crate::network::Network;

    #[test]
    fn first_response_stream_receives_frames_buffered_during_setup() {
        let (outbound, _outbound_rx) = mpsc::unbounded_channel();
        let (inbound, initial_inbound) = broadcast::channel(INBOUND_CHANNEL_CAPACITY);
        let connection = WsJsonRpcConnection {
            outbound,
            state: Arc::new(WsConnectionState {
                inbound: Mutex::new(Some(inbound.clone())),
                closed: AtomicBool::new(false),
                tasks: Mutex::new(Vec::new()),
                hop_permit: Mutex::new(None),
            }),
            initial_inbound: Mutex::new(Some(initial_inbound)),
        };

        inbound
            .send(r#"{"jsonrpc":"2.0","id":1,"result":"ready"}"#.to_string())
            .expect("initial receiver keeps the frame buffered");

        let mut responses = connection.responses();
        let frame = futures::executor::block_on(responses.next()).expect("buffered response");
        assert_eq!(frame, r#"{"jsonrpc":"2.0","id":1,"result":"ready"}"#);
    }

    #[test]
    fn closing_rpc_ends_responses_and_releases_hop_capacity() {
        let capacity = Arc::new(Semaphore::new(1));
        let permit = capacity.clone().try_acquire_owned().unwrap();
        let (outbound, mut outbound_rx) = mpsc::unbounded_channel();
        let (inbound, initial_inbound) = broadcast::channel(INBOUND_CHANNEL_CAPACITY);
        let connection = WsJsonRpcConnection {
            outbound,
            state: Arc::new(WsConnectionState {
                inbound: Mutex::new(Some(inbound)),
                closed: AtomicBool::new(false),
                tasks: Mutex::new(Vec::new()),
                hop_permit: Mutex::new(Some(permit)),
            }),
            initial_inbound: Mutex::new(Some(initial_inbound)),
        };
        let mut responses = connection.responses();
        assert!(capacity.clone().try_acquire_owned().is_err());
        connection.close();
        assert!(capacity.try_acquire_owned().is_ok());
        assert_eq!(
            responses.next().now_or_never(),
            Some(None),
            "close must end an already-taken response stream immediately"
        );
        connection.send("must not be sent after close".to_string());
        assert!(outbound_rx.try_recv().is_err());
    }

    /// Every role the host says it serves has to route to that role's own chain
    /// without the test switch. `url_for` answers an unmapped genesis with the
    /// fallback URL, so a served role that the routing filter drops would connect
    /// to the People chain while the host claimed to serve something else — and
    /// `ChainContextCache` only warns when the reported genesis diverges.
    #[test]
    fn every_served_role_routes_to_its_own_chain() {
        for network in Network::value_variants() {
            let config = network.config();
            let provider = WsChainProvider::with_live_chain_routing(
                config.people_ws,
                config.live_chain_endpoints,
                false,
            );
            for entry in config.host_chain_set().chains {
                let expected = config.url_for_role(entry.identifier).unwrap_or_else(|| {
                    panic!(
                        "{} serves {:?} with no preset URL",
                        config.id, entry.identifier
                    )
                });
                assert!(
                    provider.routes(&entry.genesis_hash),
                    "{} serves {:?} but does not route it; the fallback would hide this",
                    config.id,
                    entry.identifier
                );
                assert_eq!(
                    provider.url_for(&entry.genesis_hash),
                    expected,
                    "{} serves {:?} but routes it elsewhere",
                    config.id,
                    entry.identifier
                );
            }
        }
    }

    /// The switch exists to widen routing to endpoints the preset carries without
    /// serving them as a role. No preset has one now that Asset Hub is served, so
    /// this uses a synthetic endpoint: without a case the preset cannot express,
    /// `required_for_host` and the switch could both be deleted with a green suite.
    #[test]
    fn the_test_switch_widens_routing_to_endpoints_that_are_not_roles() {
        const FALLBACK: &str = "wss://fallback.invalid";
        let optional = [ChainEndpoint {
            genesis: [0x5a; 32],
            ws: "wss://optional.invalid",
            required_for_host: false,
        }];

        let gated = WsChainProvider::with_live_chain_routing(FALLBACK, &optional, false);
        let widened = WsChainProvider::with_live_chain_routing(FALLBACK, &optional, true);

        assert_eq!(
            gated.url_for(&optional[0].genesis),
            FALLBACK,
            "an endpoint that is not a role is excluded without the switch"
        );
        assert_eq!(
            widened.url_for(&optional[0].genesis),
            optional[0].ws,
            "and included with it"
        );
    }
}
