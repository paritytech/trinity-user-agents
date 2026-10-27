//! The CLI's `ChainProvider`.
//!
//! This module picks which of the network's chains a genesis hash names, and
//! `truapi-provider` connects to it on the selected backend. When a
//! connection's response stream ends, the core's chain runtime connects again.

use std::{
	collections::{BTreeMap, HashMap, hash_map::Entry},
	sync::{Arc, Mutex, PoisonError},
	time::Duration,
};

use async_trait::async_trait;
use clap::ValueEnum;
use subxt_rpcs::client::{RawRpcFuture, RawRpcSubscription, RawValue, RpcClient, RpcClientT};
use tracing::debug;
use truapi::{
	HostRpcClient,
	platform::{ChainProvider, JsonRpcConnection, ProviderError},
};
use truapi_provider::{ChainSource, EmbeddedChainProvider};

use crate::network::{ChainEndpoint, NetworkConfig};

/// How long a client from [`CliChainProvider::rpc_client`] waits for an answer,
/// as subxt's own WebSocket client does. Provider connections have no request
/// timeout, and keepalive pings hold open a node that never answers.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// The CLI's answer to which chains build as V5, for its own extrinsics.
pub struct PresetExtensionVersions(pub truapi::platform::ExtensionVersionMap);

#[async_trait]
impl truapi::platform::ExtrinsicVersionHost for PresetExtensionVersions {
	async fn transaction_extension_versions(
		&self,
	) -> Result<truapi::platform::ExtensionVersionMap, truapi::latest::GenericError> {
		Ok(self.0.clone())
	}
}

/// How the CLI reaches the chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
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
	/// routes to. Each request, and each subscription's setup, fails after
	/// [`REQUEST_TIMEOUT`] without an answer.
	pub async fn rpc_client(&self, genesis_hash: [u8; 32]) -> Result<RpcClient, ProviderError> {
		let connection = self.connect(genesis_hash).await?;
		let client = HostRpcClient::new(Arc::from(connection), crate::tokio_spawner());
		Ok(RpcClient::new(Timed(client)))
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

	/// The RPC URL a genesis routes to. Test-only, so a routing override can be
	/// asserted rather than assumed: an override written to the wrong field
	/// compiles, changes nothing, and is invisible in timings.
	#[cfg(test)]
	pub fn routed_url(&self, genesis_hash: &[u8; 32]) -> &'static str {
		self.chain_for(*genesis_hash).expect("People is routed").ws
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

#[async_trait]
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

/// `C` with [`REQUEST_TIMEOUT`] on every request and subscription setup.
struct Timed<C>(C);

impl<C: RpcClientT> RpcClientT for Timed<C> {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<RawValue>>,
	) -> RawRpcFuture<'a, Box<RawValue>> {
		Box::pin(within_request_timeout(method, self.0.request_raw(method, params)))
	}

	fn subscribe_raw<'a>(
		&'a self,
		sub: &'a str,
		params: Option<Box<RawValue>>,
		unsub: &'a str,
	) -> RawRpcFuture<'a, RawRpcSubscription> {
		Box::pin(within_request_timeout(sub, self.0.subscribe_raw(sub, params, unsub)))
	}
}

async fn within_request_timeout<T>(
	method: &str,
	call: RawRpcFuture<'_, T>,
) -> Result<T, subxt_rpcs::Error> {
	tokio::time::timeout(REQUEST_TIMEOUT, call).await.unwrap_or_else(|_| {
		Err(subxt_rpcs::Error::Client(Box::new(std::io::Error::new(
			std::io::ErrorKind::TimedOut,
			format!("{method}: no answer within {}s", REQUEST_TIMEOUT.as_secs()),
		))))
	})
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
}

#[cfg(test)]
mod tests {
	use futures_util::{SinkExt, StreamExt};
	use tokio_tungstenite::tungstenite::Message;

	use super::*;
	use crate::network::Network;

	/// A node that holds every request open, as one that lost the request does.
	struct NeverAnswers;

	impl RpcClientT for NeverAnswers {
		fn request_raw<'a>(
			&'a self,
			_method: &'a str,
			_params: Option<Box<RawValue>>,
		) -> RawRpcFuture<'a, Box<RawValue>> {
			Box::pin(futures::future::pending())
		}

		fn subscribe_raw<'a>(
			&'a self,
			_sub: &'a str,
			_params: Option<Box<RawValue>>,
			_unsub: &'a str,
		) -> RawRpcFuture<'a, RawRpcSubscription> {
			Box::pin(futures::future::pending())
		}
	}

	/// Keepalive pings hold such a connection open, so without a request timeout
	/// a command reading the chain, or waiting for its transaction to be
	/// watched, would wait forever rather than fail.
	#[tokio::test(start_paused = true)]
	async fn requests_and_subscriptions_the_node_never_answers_fail() {
		use subxt_rpcs::client::rpc_params;

		let rpc = RpcClient::new(Timed(NeverAnswers));
		let request = rpc.request::<serde_json::Value>("state_getStorage", rpc_params![]);
		let Err(request) =
			tokio::time::timeout(REQUEST_TIMEOUT * 2, request).await.expect("gives up")
		else {
			panic!("nothing answered");
		};
		let subscription = rpc.subscribe::<serde_json::Value>(
			"author_submitAndWatchExtrinsic",
			rpc_params![],
			"author_unwatchExtrinsic",
		);
		let Err(subscription) =
			tokio::time::timeout(REQUEST_TIMEOUT * 2, subscription).await.expect("gives up")
		else {
			panic!("nothing answered");
		};

		assert_eq!(
			[request.to_string(), subscription.to_string()],
			[
				"RPC error: client error: state_getStorage: no answer within 60s",
				"RPC error: client error: author_submitAndWatchExtrinsic: no answer within 60s",
			]
		);
	}

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

	/// Over RPC, a catalog chain the network does not route, such as its relay,
	/// must reach People's RPC node rather than start the light client.
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

	/// Without a People route there is nothing to fall back to. Passing People's
	/// unregistered genesis on would start the light client from the catalog.
	#[tokio::test]
	async fn a_network_without_a_people_route_refuses_the_sentinel() {
		let mut network = Network::default().config();
		network.live_chain_endpoints = &[];
		let provider = CliChainProvider::with_live_chain_routing(&network, false);

		let Err(error) = provider.connect([0; 32]).await else {
			panic!("nothing is routed, so nothing may connect");
		};
		assert_eq!(
			error.to_string(),
			format!("no chain registered for genesis 0x{}", "00".repeat(32))
		);
	}

	/// A chain dropped between uses would sync again from its checkpoint and
	/// start with no peers.
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
	/// chain while the host claimed to serve something else, and
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

	/// The genesis check of a rerouted endpoint must ask that URL, whatever the
	/// network's routing would do with the hash, and nothing else.
	#[test]
	fn an_endpoint_provider_reaches_only_its_endpoint() {
		let network = people_only("wss://people.invalid".to_owned());
		let optional: &'static ChainEndpoint = Box::leak(Box::new(ChainEndpoint {
			genesis: [0x5a; 32],
			ws: "wss://optional.invalid",
			required_for_host: false,
		}));

		let alone = CliChainProvider::for_endpoint(&network, optional);

		assert_eq!(alone.routed_url(&optional.genesis), optional.ws);
		assert!(alone.chain_for(network.people_genesis).is_none(), "People is not this endpoint");
	}

	/// Ignored by default; needs network access to paseo-next-v2's chains. The
	/// light client syncs from its bundled checkpoint first, so it runs longer.
	///
	/// ```sh
	/// cargo test -p truapi-host-cli --bin truapi-host reaches_paseo -- --ignored --nocapture
	/// ```
	#[tokio::test]
	#[ignore = "needs network access to the preset's chains"]
	async fn rpc_reaches_paseo_next_v2() {
		reach_paseo_next_v2(ChainBackend::Rpc).await;
	}

	#[tokio::test]
	#[ignore = "needs network access to the preset's chains"]
	async fn the_light_client_reaches_paseo_next_v2() {
		reach_paseo_next_v2(ChainBackend::LightClient).await;
	}

	/// Every consumer this provider serves reaches its own chain on `backend`, and
	/// a closed connection is replaced.
	async fn reach_paseo_next_v2(backend: ChainBackend) {
		use subxt_rpcs::client::rpc_params;

		use crate::{
			bulletin_lookup::{BitswapRpc, BlobSource, SourceError},
			dotns_read::AssetHubReader,
		};

		let network = NetworkConfig { chain_backend: backend, ..Network::PaseoNextV2.config() };
		let provider = Arc::new(CliChainProvider::new(&network));
		let reported_genesis = async |genesis: [u8; 32]| -> String {
			let rpc = provider.rpc_client(genesis).await.expect("connect");
			rpc.request("chainSpec_v1_genesisHash", rpc_params![])
				.await
				.expect("genesis hash")
		};

		for genesis in [network.people_genesis, network.asset_hub_genesis, network.bulletin_genesis]
		{
			assert_eq!(reported_genesis(genesis).await, format!("0x{}", hex::encode(genesis)));
		}
		provider
			.connect(network.people_genesis)
			.await
			.expect("connect to People")
			.close();
		assert_eq!(
			reported_genesis(network.people_genesis).await,
			format!("0x{}", hex::encode(network.people_genesis)),
			"a new People connection replaces the closed one"
		);

		let mut reader = AssetHubReader::connect(&network).await.expect("connect to Asset Hub");
		assert!(reader.timestamp_secs().await.expect("Asset Hub time") > 0);
		assert!(reader.dotns_identity(&[0x11; 32]).await.expect("dotNS read").is_none());

		// A light client may have no Bulletin peers yet, so a transient failure is
		// asked again, as a preimage lookup does.
		let bitswap = BitswapRpc::new(Arc::clone(&provider), network.bulletin_genesis);
		let unstored = truapi::preimage_cid(&[0x22; 32]);
		for _ in 0..10 {
			match bitswap.get(&unstored).await {
				Ok(None) => return,
				Ok(Some(_)) => panic!("nobody stored {unstored}"),
				Err(SourceError::Transient(reason)) => {
					eprintln!("bitswap_v1_get: {reason}; asking again");
					tokio::time::sleep(std::time::Duration::from_secs(6)).await;
				},
				Err(SourceError::Permanent(reason)) => panic!("{reason}"),
			}
		}
		panic!("Bulletin never answered bitswap_v1_get");
	}
}
