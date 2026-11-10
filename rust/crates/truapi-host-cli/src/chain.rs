//! The CLI's `ChainProvider` and `HopProvider`.
//!
//! This module picks which of the network's chains a genesis hash names, and
//! `truapi-provider` connects to it on the selected backend. When a
//! connection's response stream ends, the core's chain runtime connects again.
//!
//! HOP relays, the stack's `hop` service, are private WSS endpoints for the
//! Bulletin chain rather than chain nodes, so they are reached over WebSocket
//! on both backends and capped at [`MAX_HOP_CONNECTIONS`] at a time.

use std::{
	collections::BTreeMap,
	sync::{Arc, Mutex, PoisonError},
};

use clap::ValueEnum;
use futures::{
	Stream, StreamExt,
	stream::{self, BoxStream, FusedStream},
};
use subxt_rpcs::client::RpcClient;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::debug;
use truapi::{
	HostRpcClient, latest as api,
	platform::{ChainProvider, HopProvider, JsonRpcConnection, ProviderError},
};
use truapi_provider::{
	ChainHealth, ChainLifecycle, ChainPhase, ChainSource, EmbeddedChainProvider, StallReason,
};

use crate::network::{ChainEndpoint, Stack};

/// Private HOP connections open at once across every relay.
const MAX_HOP_CONNECTIONS: usize = 8;

/// The CLI's answer to which chains build as V5, for its own extrinsics.
pub struct PresetExtensionVersions(pub truapi::platform::ExtensionVersionMap);

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
	hop: HopRelays,
}

enum Backend {
	/// Each routed chain registered as its endpoint's RPC node.
	Rpc(Box<EmbeddedChainProvider>),
	/// The process's light client for the network with this product
	/// namespace; see [`light_client`].
	LightClient(String),
}

/// The stack's `hop` service: the exact WSS relays trusted for its Bulletin
/// chain. A relay trusted for another Bulletin is not trusted for this one,
/// so every lookup is keyed by the Bulletin genesis.
struct HopRelays {
	bulletin_genesis: [u8; 32],
	/// One RPC-node provider per relay URL, since a provider registers a single
	/// node per genesis. None of them starts a light client.
	relays: Vec<(String, EmbeddedChainProvider)>,
	connections: Arc<Semaphore>,
}

impl HopRelays {
	fn allowed(&self, bulletin_genesis_hash: [u8; 32]) -> Vec<String> {
		if bulletin_genesis_hash != self.bulletin_genesis {
			return Vec::new();
		}
		self.relays.iter().map(|(url, _)| url.clone()).collect()
	}
}

impl CliChainProvider {
	/// A provider for `network`'s chains over its chain backend, and for its
	/// HOP relays over WebSocket.
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
		let bulletin_genesis = network.genesis().bulletin;
		let hop = HopRelays {
			bulletin_genesis,
			relays: network
				.hop_endpoints()
				.iter()
				.map(|url| {
					let parsed = url.parse().expect("invalid HOP URL in the stack");
					let provider = EmbeddedChainProvider::builder()
						.chain(bulletin_genesis, ChainSource::rpc_node(parsed))
						.build();
					(url.clone(), provider)
				})
				.collect(),
			connections: Arc::new(Semaphore::new(MAX_HOP_CONNECTIONS)),
		};
		Self { people_genesis: network.genesis().people, routed_endpoints, backend, hop }
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

#[async_trait::async_trait]
impl HopProvider for CliChainProvider {
	async fn allowed_hop_endpoints(
		&self,
		bulletin_genesis_hash: [u8; 32],
	) -> Result<Vec<String>, api::GenericError> {
		Ok(self.hop.allowed(bulletin_genesis_hash))
	}

	async fn connect_hop(
		&self,
		bulletin_genesis_hash: [u8; 32],
		endpoint: String,
	) -> Result<Box<dyn JsonRpcConnection>, api::GenericError> {
		let allowed = self.hop.allowed(bulletin_genesis_hash);
		if allowed.is_empty() {
			return Err(api::GenericError {
				reason: "HOP provider unavailable for this Bulletin chain".to_string(),
			});
		}
		truapi::platform::ensure_allowed_hop_endpoint(&endpoint, &allowed)?;
		let permit =
			self.hop
				.connections
				.clone()
				.try_acquire_owned()
				.map_err(|_| api::GenericError {
					reason: "HOP connection limit reached".to_string(),
				})?;
		let (_, relay) = self
			.hop
			.relays
			.iter()
			.find(|(url, _)| *url == endpoint)
			.expect("an allowed HOP endpoint is one of the relays");
		debug!(%endpoint, "HOP connect");
		let connection = relay
			.connect(bulletin_genesis_hash)
			.await
			.map_err(|err| api::GenericError { reason: err.to_string() })?;
		Ok(Box::new(HopConnection { inner: connection, permit: Mutex::new(Some(permit)) }))
	}
}

/// One private HOP lease: the relay connection plus its slot under
/// [`MAX_HOP_CONNECTIONS`], released on close or drop.
struct HopConnection {
	inner: Box<dyn JsonRpcConnection>,
	permit: Mutex<Option<OwnedSemaphorePermit>>,
}

impl JsonRpcConnection for HopConnection {
	fn send(&self, request: String) {
		self.inner.send(request);
	}

	fn responses(&self) -> BoxStream<'static, String> {
		self.inner.responses()
	}

	fn mid_message_silence(&self) -> Option<std::time::Duration> {
		self.inner.mid_message_silence()
	}

	fn close(&self) {
		self.inner.close();
		self.permit.lock().unwrap_or_else(PoisonError::into_inner).take();
	}
}

impl Drop for HopConnection {
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
