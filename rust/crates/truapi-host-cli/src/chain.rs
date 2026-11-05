//! The CLI's `ChainProvider`.
//!
//! This module picks which of the network's chains a genesis hash names, and
//! `truapi-provider` connects to it on the selected backend. When a
//! connection's response stream ends, the core's chain runtime connects again.

use std::{
	collections::{BTreeMap, HashMap, hash_map::Entry},
	sync::{Arc, Mutex, PoisonError},
};

use clap::ValueEnum;
use futures::{
	Stream, StreamExt,
	stream::{self, FusedStream},
};
use subxt_rpcs::client::RpcClient;
use tracing::debug;
use truapi::{
	HostRpcClient,
	platform::{ChainProvider, JsonRpcConnection, ProviderError},
};
use truapi_provider::{
	ChainHealth, ChainLifecycle, ChainPhase, ChainSource, EmbeddedChainProvider, StallReason,
};

use crate::network::{ChainEndpoint, NetworkConfig};

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

/// How the CLI reaches the chains.
#[derive(Clone, ValueEnum, PartialEq, Debug, Copy)]
pub enum ChainBackend {
	/// WebSocket JSON-RPC to the network's nodes.
	Rpc,
	/// The embedded smoldot light client, which needs no RPC node. A chain's
	/// first request waits for it to sync.
	LightClient,
}

/// Chain provider shared by the host runtime, dotNS reads and Bulletin
/// preimage lookups.
///
/// Routing is the same on both backends. Every role the network serves (People,
/// Bulletin and Asset Hub) is routed to its own chain, and the test switch also
/// routes endpoints the network carries without serving them. The all-zero SSO
/// sentinel and any other genesis are answered by People.
pub struct CliChainProvider {
	network: NetworkConfig,
	live_chain_routing: bool,
	backend: Backend,
}

enum Backend {
	/// Each routed chain registered as its endpoint's RPC node.
	Rpc(Box<EmbeddedChainProvider>),
	/// The process's light client for the network; see [`LightClient`].
	LightClient,
}

impl CliChainProvider {
	/// Reads the test switch from the `E2E_LIVE_CHAIN` environment variable.
	pub fn new(network: &NetworkConfig) -> Self {
		let live_chain_routing = std::env::var("E2E_LIVE_CHAIN").as_deref() == Ok("1");
		Self::with_live_chain_routing(network, live_chain_routing)
	}

	fn with_live_chain_routing(network: &NetworkConfig, live_chain_routing: bool) -> Self {
		let backend = match network.chain_backend {
			ChainBackend::Rpc => Backend::Rpc(Box::new(
				routed_endpoints(network, live_chain_routing)
					.fold(EmbeddedChainProvider::builder(), |builder, endpoint| {
						let url = endpoint
							.ws
							.parse()
							.expect("chain URLs are validated when the network config is resolved");
						builder.chain(endpoint.genesis, ChainSource::rpc_node(url))
					})
					.build(),
			)),
			ChainBackend::LightClient => Backend::LightClient,
		};
		Self { network: *network, live_chain_routing, backend }
	}

	/// A provider for `endpoint` alone, over RPC, so a check reaches exactly the
	/// URL it names on the transport the routed chains use.
	pub fn for_endpoint(network: &NetworkConfig, endpoint: &'static ChainEndpoint) -> Self {
		let alone = NetworkConfig {
			live_chain_endpoints: std::slice::from_ref(endpoint),
			chain_backend: ChainBackend::Rpc,
			..*network
		};
		Self::with_live_chain_routing(&alone, true)
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
		if let (Backend::LightClient, Some(chain)) = (&self.backend, self.chain_for(genesis_hash)) {
			LightClient::shared(self.network.id)?.wait_until_synced(chain.genesis).await?;
		}
		let client = HostRpcClient::new(Arc::from(connection), crate::tokio_spawner());
		Ok(RpcClient::new(client))
	}

	/// The routed endpoint that answers `genesis_hash`: its own, otherwise
	/// People's, and none when People is not routed either.
	///
	/// Only routed hashes reach the provider. Asked for any other, it would look
	/// the hash up in its bundled catalog and start the light client, whichever
	/// backend was selected.
	fn chain_for(&self, genesis_hash: [u8; 32]) -> Option<&'static ChainEndpoint> {
		let routed = |genesis| {
			routed_endpoints(&self.network, self.live_chain_routing)
				.find(move |endpoint| endpoint.genesis == genesis)
		};
		routed(genesis_hash).or_else(|| routed(self.network.people_genesis))
	}

	/// Whether a genesis is routed to its own chain rather than to People.
	///
	/// Test-only because production has no reason to care: `chain_for` resolves
	/// either way. A test does, since People is the fallback, so asserting on the
	/// resolved URL cannot tell a routed People from a dropped one.
	#[cfg(test)]
	fn routes(&self, genesis_hash: &[u8; 32]) -> bool {
		self.chain_for(*genesis_hash)
			.is_some_and(|endpoint| endpoint.genesis == *genesis_hash)
	}

	/// Where requests for `genesis_hash` go, for messages: the URL of the RPC
	/// node it routes to, or the light client. A test asserts a routing override
	/// through it, since an override written to the wrong field compiles,
	/// changes nothing, and is invisible in timings.
	pub fn location(&self, genesis_hash: [u8; 32]) -> &'static str {
		match (&self.backend, self.chain_for(genesis_hash)) {
			(_, None) => "unrouted",
			(Backend::Rpc(_), Some(endpoint)) => endpoint.ws,
			(Backend::LightClient, Some(_)) => "light client",
		}
	}
}

/// The endpoints routed to their own chain.
fn routed_endpoints(
	network: &NetworkConfig,
	live_chain_routing: bool,
) -> impl Iterator<Item = &'static ChainEndpoint> {
	// People remains the fallback for the SSO sentinel. Bulletin backs preimage
	// submission and Asset Hub backs PGAS claims, so all three are host
	// dependencies and must never be gated by the product-facing Chain/* switch.
	network
		.live_chain_endpoints
		.iter()
		.filter(move |endpoint| endpoint.required_for_host || live_chain_routing)
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
			Backend::LightClient => LightClient::shared(self.network.id)?.connect(chain).await,
		}
	}
}

/// The process's light client for one network. Each `EmbeddedChainProvider`
/// runs its own smoldot client, so every consumer shares this one to reuse one
/// sync and one set of peers.
struct LightClient {
	provider: EmbeddedChainProvider,
	/// One idle connection per chain for the life of the process. smoldot stops
	/// a chain once its last connection closes, and consumers connect once per
	/// use, so without it a chain would start again with no peers.
	kept_open: tokio::sync::Mutex<HashMap<[u8; 32], Box<dyn JsonRpcConnection>>>,
}

impl LightClient {
	/// The light client for `network`, with every chain of that network from
	/// `truapi-provider`'s bundled catalog registered. Smoldot starts on the
	/// first connect.
	fn shared(network: &'static str) -> Result<Arc<Self>, ProviderError> {
		static CLIENTS: Mutex<BTreeMap<&str, Arc<LightClient>>> = Mutex::new(BTreeMap::new());

		let mut clients = CLIENTS.lock().unwrap_or_else(PoisonError::into_inner);
		if let Some(client) = clients.get(network) {
			return Ok(Arc::clone(client));
		}
		let (builder, _) = EmbeddedChainProvider::builder().add_network(network)?;
		let client = Arc::new(Self { provider: builder.build(), kept_open: Default::default() });
		clients.insert(network, Arc::clone(&client));
		Ok(client)
	}

	async fn connect(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
		if let Entry::Vacant(slot) = self.kept_open.lock().await.entry(genesis_hash) {
			slot.insert(self.provider.connect(genesis_hash).await?);
		}
		self.provider.connect(genesis_hash).await
	}

	/// Waits until the light client has synced `genesis_hash`; see
	/// [`synced_or_stalled`]. Connect to the chain first, since only a running
	/// chain has a lifecycle.
	async fn wait_until_synced(&self, genesis_hash: [u8; 32]) -> Result<(), ProviderError> {
		let relay = match self.provider.relay_of(genesis_hash) {
			Some(relay) => self.provider.lifecycle(relay)?,
			None => stream::empty().boxed(),
		};
		synced_or_stalled(genesis_hash, self.provider.lifecycle(genesis_hash)?, relay.fuse()).await
	}
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
