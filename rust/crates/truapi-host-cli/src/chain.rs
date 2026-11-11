//! The CLI's `ChainProvider`.
//!
//! This module picks which of the network's chains a genesis hash names, and
//! `truapi-provider` connects to it on the selected backend. When a
//! connection's response stream ends, the core's chain runtime connects again.

use std::{
	collections::BTreeMap,
	sync::{
		Arc, Mutex, PoisonError,
		atomic::{AtomicBool, Ordering},
	},
};

use clap::ValueEnum;
use futures::{
	Stream, StreamExt,
	stream::{self, BoxStream, FusedStream},
};
use futures_util::SinkExt;
use subxt_rpcs::client::RpcClient;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, warn};
use truapi::{
	HostRpcClient, latest as api,
	platform::{ChainProvider, HopProvider, JsonRpcConnection, ProviderError},
};
use truapi_provider::{
	ChainHealth, ChainLifecycle, ChainPhase, ChainSource, EmbeddedChainProvider, StallReason,
};

use crate::network::{ChainEndpoint, Stack};

/// The CLI's answer to which chains build as V5, for its own extrinsics.
pub struct PresetExtensionVersions(pub truapi::platform::ExtensionVersionMap);

/// Broadcast backlog for inbound HOP JSON-RPC frames per connection.
const INBOUND_CHANNEL_CAPACITY: usize = 1024;
const MAX_HOP_CONNECTIONS: usize = 8;

#[async_trait::async_trait]
impl truapi::platform::ExtrinsicVersionHost for PresetExtensionVersions {
	async fn transaction_extension_versions(
		&self,
	) -> Result<truapi::platform::ExtensionVersionMap, truapi::latest::GenericError> {
		Ok(self.0.clone())
	}
}

/// How the CLI reaches the chains. Displayed as its `--chain-backend` value.
#[derive(Debug, Clone, Copy, PartialEq, ValueEnum, derive_more::Display)]
pub enum ChainBackend {
	/// WebSocket JSON-RPC to the network's nodes.
	#[display("rpc")]
	Rpc,
	/// The embedded smoldot light client, which needs no RPC node. A chain's
	/// first request waits for it to sync.
	#[display("light-client")]
	LightClient,
}

/// Chain provider shared by the host runtime, dotNS reads and Bulletin
/// preimage lookups.
///
/// Routing is the same on both backends. Each role the network serves (People,
/// Bulletin and Asset Hub) is routed to its own chain. The all-zero SSO
/// sentinel and any other genesis are answered by People.
pub struct CliChainProvider {
	people_genesis: [u8; 32],
	routed_endpoints: Vec<ChainEndpoint>,
	backend: Backend,
	bulletin_genesis: [u8; 32],
	hop_endpoints: Vec<String>,
	hop_connections: Arc<Semaphore>,
}

enum Backend {
	/// Each routed chain registered as its endpoint's RPC node.
	Rpc(Box<EmbeddedChainProvider>),
	/// The process's light client for the network with this product
	/// namespace; see [`light_client`].
	LightClient(String),
}

impl CliChainProvider {
	/// A provider for `network`'s chains over its chain backend.
	pub fn new(network: &Stack) -> Self {
		let routed_endpoints: Vec<ChainEndpoint> = network.chains().iter().cloned().collect();
		let backend = match network.chain_backend() {
			ChainBackend::Rpc => Backend::Rpc(Box::new(
				routed_endpoints
					.iter()
					.fold(EmbeddedChainProvider::builder(), |builder, endpoint| {
						let url = endpoint.ws.parse().expect("invalid chain URL in the stack");
						builder.chain(endpoint.genesis, ChainSource::rpc_node(url))
					})
					.build(),
			)),
			ChainBackend::LightClient => Backend::LightClient(network.namespace().to_owned()),
		};
		Self {
			people_genesis: network.genesis().people,
			routed_endpoints,
			backend,
			bulletin_genesis: network.genesis().bulletin,
			hop_endpoints: network
				.hop_endpoints()
				.iter()
				.map(|endpoint| endpoint.ws.to_owned())
				.collect(),
			hop_connections: Arc::new(Semaphore::new(MAX_HOP_CONNECTIONS)),
		}
	}

	/// A subxt RPC client over a new connection to the chain `genesis_hash`
	/// routes to. A request fails once the connection has been silent for
	/// [`HostRpcClient`]'s deadline while it waits.
	///
	/// On the light client this first waits for the chain to sync. smoldot sends
	/// nothing on the connection until then, so a first request sent during a
	/// long sync would run into that deadline.
	pub async fn rpc_client(&self, genesis_hash: [u8; 32]) -> Result<RpcClient, ProviderError> {
		let connection = self.connect(genesis_hash).await?;
		if let (Backend::LightClient(network_id), Some(chain)) =
			(&self.backend, self.chain_for(genesis_hash))
		{
			wait_until_synced(&*light_client(network_id)?, chain.genesis).await?;
		}
		let client = HostRpcClient::new(Arc::from(connection), crate::tokio_spawner());
		Ok(RpcClient::new(client))
	}

	/// The routed endpoint that answers `genesis_hash`: its own, otherwise
	/// People's, and none when People is not routed either.
	fn chain_for(&self, genesis_hash: [u8; 32]) -> Option<&ChainEndpoint> {
		let routed =
			|genesis| self.routed_endpoints.iter().find(|endpoint| endpoint.genesis == genesis);
		routed(genesis_hash).or_else(|| routed(self.people_genesis))
	}

	/// Where requests for `genesis_hash` go, for messages: the URL of the RPC
	/// node it routes to, or the light client.
	pub fn location(&self, genesis_hash: [u8; 32]) -> &str {
		match (&self.backend, self.chain_for(genesis_hash)) {
			(_, None) => "unrouted",
			(Backend::Rpc(_), Some(endpoint)) => &endpoint.ws,
			(Backend::LightClient(_), Some(_)) => "light client",
		}
	}
}

#[async_trait::async_trait]
impl ChainProvider for CliChainProvider {
	async fn connect(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
		let chain = self
			.chain_for(genesis_hash)
			.ok_or(ProviderError::UnknownGenesis { genesis: genesis_hash })?
			.genesis;
		debug!(genesis = %hex::encode(genesis_hash), chain = %hex::encode(chain), "chain connect");
		match &self.backend {
			Backend::Rpc(provider) => provider.connect(chain).await,
			Backend::LightClient(network_id) => light_client(network_id)?.connect(chain).await,
		}
	}
}

// HOP remains an independent, pinned WSS transport on both chain backends.
// The generic Bulletin provider cannot serve native Chat attachment uploads.
#[async_trait::async_trait]
impl HopProvider for CliChainProvider {
	async fn allowed_hop_endpoints(
		&self,
		bulletin_genesis_hash: [u8; 32],
	) -> Result<Vec<String>, api::GenericError> {
		Ok(if bulletin_genesis_hash == self.bulletin_genesis {
			self.hop_endpoints.clone()
		} else {
			Vec::new()
		})
	}

	async fn connect_hop(
		&self,
		bulletin_genesis_hash: [u8; 32],
		endpoint: String,
	) -> Result<Box<dyn JsonRpcConnection>, api::GenericError> {
		if bulletin_genesis_hash != self.bulletin_genesis {
			return Err(api::GenericError {
				reason: "HOP provider unavailable for this Bulletin chain".to_string(),
			});
		}
		truapi::platform::ensure_allowed_hop_endpoint(&endpoint, &self.hop_endpoints)?;
		let permit =
			self.hop_connections
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

/// One HOP WebSocket lease, separate from the generic chain provider.
///
/// Closing or dropping it ends response streams, stops both socket tasks, and
/// releases connection capacity. The first response stream retains frames that
/// arrived before subscription setup.
struct WsJsonRpcConnection {
	outbound: mpsc::UnboundedSender<Message>,
	state: Arc<WsConnectionState>,
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
		for task in self.tasks.lock().expect("websocket task mutex poisoned").drain(..) {
			task.abort();
		}
		self.inbound.lock().expect("websocket inbound mutex poisoned").take();
		self.hop_permit.lock().expect("HOP lease mutex poisoned").take();
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
				if let Some(response) = response &&
					let Some(inbound) = reader_state
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
		if !self.state.closed.load(Ordering::Acquire) {
			let _ = self.outbound.send(Message::Text(request));
		}
	}

	fn responses(&self) -> BoxStream<'static, String> {
		let receiver = self
			.initial_inbound
			.lock()
			.expect("initial HOP response receiver mutex poisoned")
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
			return stream::empty().boxed();
		};
		stream::unfold(receiver, |mut receiver| async move {
			loop {
				match receiver.recv().await {
					Ok(response) => return Some((response, receiver)),
					Err(broadcast::error::RecvError::Closed) => return None,
					Err(broadcast::error::RecvError::Lagged(dropped)) => {
						warn!(dropped, "HOP response subscriber lagged");
					},
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

/// The process's light client for `network`, its chains kept running between
/// uses. Shared, because each `EmbeddedChainProvider` runs its own smoldot.
fn light_client(network: &str) -> Result<Arc<EmbeddedChainProvider>, ProviderError> {
	static CLIENTS: Mutex<BTreeMap<String, Arc<EmbeddedChainProvider>>> =
		Mutex::new(BTreeMap::new());

	let mut clients = CLIENTS.lock().unwrap_or_else(PoisonError::into_inner);
	if let Some(client) = clients.get(network) {
		return Ok(Arc::clone(client));
	}
	let (builder, _) = EmbeddedChainProvider::builder().add_network(network)?;
	let client = Arc::new(builder.keep_chains_running().build());
	clients.insert(network.to_owned(), Arc::clone(&client));
	Ok(client)
}

/// Waits until `light` has synced `genesis_hash`; see [`synced_or_stalled`].
/// Connect to the chain first, since only a running chain has a lifecycle.
async fn wait_until_synced(
	light: &EmbeddedChainProvider,
	genesis_hash: [u8; 32],
) -> Result<(), ProviderError> {
	let relay = match light.relay_of(genesis_hash) {
		Some(relay) => light.lifecycle(relay)?,
		None => stream::empty().boxed(),
	};
	synced_or_stalled(genesis_hash, light.lifecycle(genesis_hash)?, relay.fuse()).await
}

/// Waits until the chain is synced, which smoldot reports as `Ready`. A stall of
/// the chain or of its relay fails, since a parachain never reports its relay's.
async fn synced_or_stalled(
	genesis: [u8; 32],
	chain_lifecycle: impl Stream<Item = ChainLifecycle>,
	relay_lifecycle: impl FusedStream<Item = ChainLifecycle>,
) -> Result<(), ProviderError> {
	let (mut chain, mut relay) = (std::pin::pin!(chain_lifecycle), std::pin::pin!(relay_lifecycle));
	loop {
		tokio::select! {
			state = chain.next() => {
				let state = state.ok_or(ProviderError::NotRunning { genesis })?;
				healthy(genesis, "chain", state.health)?;
				if state.phase == ChainPhase::Ready {
					return Ok(());
				}
			},
			Some(state) = relay.next() => healthy(genesis, "the relay of chain", state.health)?,
		}
	}
}

/// Fails when `health` reports a stall of `which`, named with `genesis`.
fn healthy(genesis: [u8; 32], which: &str, health: ChainHealth) -> Result<(), ProviderError> {
	let ChainHealth::Stalled { reason } = health else {
		return Ok(());
	};
	let why = match reason {
		StallReason::NoPeers => "it has no peers",
		StallReason::NoProgress => "its warp sync stopped advancing",
	};
	Err(ProviderError::Host {
		reason: format!("the light client stalled on {which} 0x{}: {why}", hex::encode(genesis)),
	})
}

#[cfg(test)]
mod tests;
