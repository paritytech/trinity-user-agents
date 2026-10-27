//! The CLI's `ChainProvider`, behind every chain connection the hosts open.
//!
//! The headless hosts reach the real People-chain statement store, the same
//! chain an iOS/web client uses, plus Bulletin and Asset Hub. This module picks
//! which of the network's chains a genesis hash names, and every `connect` goes
//! through `truapi-provider` on the backend `--chain-backend` selects. The
//! provider owns keepalive, shutdown, and ending the response stream on
//! disconnect. The runtime's `HostRpcClient` sits on top, speaks
//! statement-store RPC, and connects again once that stream ends.

use std::{
	collections::{BTreeMap, HashMap, hash_map::Entry},
	sync::{Arc, Mutex, PoisonError},
};

use async_trait::async_trait;
use clap::ValueEnum;
use subxt_rpcs::client::RpcClient;
use tracing::debug;
use truapi::{
	HostRpcClient,
	platform::{ChainProvider, JsonRpcConnection, ProviderError},
};
use truapi_provider::{ChainSource, EmbeddedChainProvider};

use crate::network::{ChainEndpoint, NetworkConfig};

/// How the CLI reaches the chains.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ChainBackend {
	/// WebSocket JSON-RPC to the network's nodes.
	#[default]
	Rpc,
	/// The embedded smoldot light client, which needs no RPC node. A chain's
	/// first request waits for it to sync.
	LightClient,
}

/// Chain provider shared by the host runtime, dotNS reads and Bulletin
/// preimage lookups.
///
/// Routing is the same on both backends. Every role the network serves —
/// People, Bulletin and Asset Hub — is routed to its own chain; the test switch
/// only widens routing to endpoints the network carries without serving them as
/// a role. The all-zero genesis (the headless SSO sentinel) and any unrouted
/// genesis are answered by People.
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
				routes(network, live_chain_routing)
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

	/// A subxt RPC client over a new connection to the chain `genesis_hash`
	/// routes to.
	pub async fn rpc_client(&self, genesis_hash: [u8; 32]) -> Result<RpcClient, ProviderError> {
		let connection = self.connect(genesis_hash).await?;
		Ok(RpcClient::new(HostRpcClient::new(Arc::from(connection), crate::tokio_spawner())))
	}

	/// The chain that answers `genesis_hash`: its own when routed, People
	/// otherwise.
	///
	/// Only routed hashes reach the provider. Asked for any other, it would look
	/// the hash up in its bundled catalog and start the light client, whichever
	/// backend was selected.
	fn chain_for(&self, genesis_hash: [u8; 32]) -> [u8; 32] {
		if routes(&self.network, self.live_chain_routing)
			.any(|endpoint| endpoint.genesis == genesis_hash)
		{
			genesis_hash
		} else {
			self.network.people_genesis
		}
	}

	/// Whether a genesis is routed to its own chain rather than to People.
	///
	/// Test-only because production has no reason to care: `chain_for` resolves
	/// either way. A test does, since People is the fallback, so asserting on the
	/// resolved URL cannot tell a routed People from a dropped one.
	#[cfg(test)]
	fn routes(&self, genesis_hash: &[u8; 32]) -> bool {
		self.chain_for(*genesis_hash) == *genesis_hash
	}

	/// The RPC URL a genesis routes to. Test-only, so a routing override can be
	/// asserted rather than assumed: an override written to the wrong field
	/// compiles, changes nothing, and is invisible in timings.
	#[cfg(test)]
	pub fn routed_url(&self, genesis_hash: &[u8; 32]) -> &'static str {
		let chain = self.chain_for(*genesis_hash);
		routes(&self.network, self.live_chain_routing)
			.find(|endpoint| endpoint.genesis == chain)
			.map(|endpoint| endpoint.ws)
			.expect("People is routed")
	}
}

/// The endpoints routed to their own chain.
fn routes(
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

#[async_trait]
impl ChainProvider for CliChainProvider {
	async fn connect(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
		let chain = self.chain_for(genesis_hash);
		debug!(genesis = %hex::encode(genesis_hash), chain = %hex::encode(chain), "chain connect");
		match &self.backend {
			Backend::Rpc(provider) => provider.connect(chain).await,
			Backend::LightClient => LightClient::shared(self.network.id)?.connect(chain).await,
		}
	}
}

/// The process's light client for one network, shared by every consumer.
///
/// Each `EmbeddedChainProvider` runs its own smoldot client, and smoldot stops
/// a chain once its last connection closes. Sharing one provider per network,
/// holding one idle connection per chain, lets the runtime, dotNS reads and
/// preimage lookups reuse one sync and one set of peers.
struct LightClient {
	provider: EmbeddedChainProvider,
	/// One idle connection per chain for the life of the process. Consumers
	/// connect once per use, so without it a chain would stop between uses and
	/// start again with no peers to broadcast a statement to.
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
}

#[cfg(test)]
mod tests {
	use futures_util::{SinkExt, StreamExt};
	use tokio_tungstenite::tungstenite::Message;

	use super::*;
	use crate::network::Network;

	/// A network whose only route is People at `people_ws`.
	fn people_only(people_ws: String) -> NetworkConfig {
		let mut network = Network::default().config();
		network.live_chain_endpoints = Box::leak(Box::new([ChainEndpoint {
			genesis: network.people_genesis,
			ws: Box::leak(people_ws.into_boxed_str()),
			required_for_host: true,
		}]));
		network
	}

	/// A loopback server that sends `open` on every connection, then closes it.
	async fn serve_one_frame_per_connection() -> String {
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
			.await
			.expect("bind a loopback listener");
		let url = format!("ws://{}", listener.local_addr().expect("listener address"));
		tokio::spawn(async move {
			loop {
				let (socket, _) = listener.accept().await.expect("accept the client");
				let mut socket = tokio_tungstenite::accept_async(socket)
					.await
					.expect("complete the WebSocket handshake");
				socket.send(Message::text("open")).await.expect("send the first frame");
				socket.close(None).await.expect("close the socket");
			}
		});
		url
	}

	/// The runtime keeps a chain connection open across calls and replaces it
	/// only once its response stream ends. A stream that outlived its socket
	/// would leave every later request on that chain waiting forever.
	#[tokio::test]
	async fn a_closed_socket_ends_the_stream_and_a_new_connection_replaces_it() {
		let network = people_only(serve_one_frame_per_connection().await);
		let provider = CliChainProvider::with_live_chain_routing(&network, false);

		let closed = provider.connect([0; 32]).await.expect("connect to the loopback server");
		let mut responses = closed.responses();
		assert_eq!(responses.next().await.as_deref(), Some("open"));
		let end = tokio::time::timeout(std::time::Duration::from_secs(5), responses.next())
			.await
			.expect("the response stream must end once the socket closes");
		assert_eq!(end, None);

		let replacement = provider.connect([0; 32]).await.expect("connect again");
		assert_eq!(replacement.responses().next().await.as_deref(), Some("open"));
	}

	/// The provider resolves a genesis it was not given from its bundled
	/// catalog, on the light client. Over RPC, a product naming a catalog chain
	/// the network does not route, such as its relay, must still reach the RPC
	/// node that answers for People.
	#[tokio::test]
	async fn an_unrouted_catalog_chain_stays_on_rpc() {
		let network = people_only(serve_one_frame_per_connection().await);
		let (_, catalog) = truapi_provider::EmbeddedChainProviderBuilder::new()
			.add_network(network.id)
			.expect("the default network is bundled");
		let provider = CliChainProvider::with_live_chain_routing(&network, false);

		let connection = provider.connect(catalog.relay).await.expect("connect to People");
		let first =
			tokio::time::timeout(std::time::Duration::from_secs(5), connection.responses().next())
				.await
				.expect("the loopback server answers at once");
		assert_eq!(first.as_deref(), Some("open"));
	}

	/// Every consumer connects once per use, and smoldot stops a chain when its
	/// last connection closes. A chain dropped between uses would sync again
	/// from its checkpoint and start with no peers.
	#[tokio::test]
	async fn a_light_client_chain_keeps_running_after_its_connection_closes() {
		let mut network = Network::default().config();
		network.chain_backend = ChainBackend::LightClient;
		let provider = CliChainProvider::with_live_chain_routing(&network, false);

		drop(provider.connect(network.people_genesis).await.expect("add People"));

		let light = LightClient::shared(network.id).expect("the default network is bundled");
		assert!(light.provider.is_connected(network.people_genesis));
	}

	/// Every role the host says it serves has to route to that role's own chain
	/// without the test switch. An unmapped genesis is answered by People, so a
	/// served role that the routing filter drops would connect to the People
	/// chain while the host claimed to serve something else — and
	/// `ChainContextCache` only warns when the reported genesis diverges.
	#[test]
	fn every_served_role_routes_to_its_own_chain() {
		for network in Network::value_variants() {
			let config = network.config();
			let provider = CliChainProvider::with_live_chain_routing(&config, false);
			for entry in config.host_chain_set().chains {
				let expected = config.url_for_role(entry.identifier).unwrap_or_else(|| {
					panic!("{} serves {:?} with no preset URL", config.id, entry.identifier)
				});
				assert!(
					provider.routes(&entry.genesis_hash),
					"{} serves {:?} but does not route it; the fallback would hide this",
					config.id,
					entry.identifier
				);
				assert_eq!(
					provider.routed_url(&entry.genesis_hash),
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
		const PEOPLE: &str = "wss://people.invalid";
		let mut network = people_only(PEOPLE.to_owned());
		let optional = ChainEndpoint {
			genesis: [0x5a; 32],
			ws: "wss://optional.invalid",
			required_for_host: false,
		};
		network.live_chain_endpoints =
			Box::leak(Box::new([network.live_chain_endpoints[0], optional]));

		let gated = CliChainProvider::with_live_chain_routing(&network, false);
		let widened = CliChainProvider::with_live_chain_routing(&network, true);

		assert_eq!(
			gated.routed_url(&optional.genesis),
			PEOPLE,
			"an endpoint that is not a role is excluded without the switch"
		);
		assert_eq!(widened.routed_url(&optional.genesis), optional.ws, "and included with it");
	}
}
