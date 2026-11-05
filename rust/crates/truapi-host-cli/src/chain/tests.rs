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
	assert_eq!(error.to_string(), format!("no chain registered for genesis 0x{}", "00".repeat(32)));
}

const PARACHAIN: [u8; 32] = [0x11; 32];

fn lifecycle(phase: ChainPhase, health: ChainHealth) -> ChainLifecycle {
	ChainLifecycle { phase, peers: 1, health }
}

/// `updates`, then nothing more while the chain keeps running.
fn then_quiet(updates: Vec<ChainLifecycle>) -> impl Stream<Item = ChainLifecycle> {
	stream::iter(updates).chain(stream::pending())
}

/// What `synced_or_stalled` has decided for `PARACHAIN` once the updates are
/// delivered and nothing else changes, or `None` while it still waits.
fn decided(
	chain: impl Stream<Item = ChainLifecycle>,
	relay: Vec<ChainLifecycle>,
) -> Option<Result<(), ProviderError>> {
	use futures::FutureExt;

	synced_or_stalled(PARACHAIN, chain, then_quiet(relay).fuse()).now_or_never()
}

/// Only the chain's own phase lets the first request through. Its relay being
/// synced does not mean the parachain can answer yet.
#[test]
fn a_synced_relay_does_not_release_its_parachain() {
	let connecting = lifecycle(ChainPhase::Connecting, ChainHealth::Ok);
	let ready = lifecycle(ChainPhase::Ready, ChainHealth::Ok);

	assert_eq!(
		[
			decided(then_quiet(vec![connecting]), vec![ready]),
			decided(then_quiet(vec![connecting, ready]), vec![]),
		],
		[None, Some(Ok(()))]
	);
}

/// A chain the light client stopped running will never be `Ready`, however
/// healthy its relay is.
#[test]
fn a_chain_that_stops_ends_the_wait() {
	let decided = decided(
		stream::iter(vec![lifecycle(ChainPhase::Connecting, ChainHealth::Ok)]),
		vec![lifecycle(ChainPhase::Ready, ChainHealth::Ok)],
	);

	assert_eq!(decided, Some(Err(ProviderError::NotRunning { genesis: PARACHAIN })));
}

/// A parachain reports a stall only from its own peers, so it looks healthy
/// while its relay has stopped syncing, and would be waited on forever.
#[test]
fn a_stalled_relay_fails_the_wait() {
	let stalled = ChainHealth::Stalled { reason: StallReason::NoProgress };

	let decided = decided(
		then_quiet(vec![lifecycle(ChainPhase::Connecting, ChainHealth::Ok)]),
		vec![lifecycle(ChainPhase::Syncing { at: 1, target: 9 }, stalled)],
	);

	assert_eq!(
		decided,
		Some(Err(ProviderError::Host {
			reason: format!(
				"the light client stalled on the relay of chain 0x{}: its warp sync stopped \
				 advancing",
				"11".repeat(32)
			),
		}))
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
				provider.location(entry.genesis_hash),
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
	network.live_chain_endpoints = Box::leak(Box::new([network.live_chain_endpoints[0], optional]));

	let gated = CliChainProvider::with_live_chain_routing(&network, false);
	let widened = CliChainProvider::with_live_chain_routing(&network, true);

	assert_eq!(
		gated.location(optional.genesis),
		PEOPLE,
		"an endpoint that is not a role is excluded without the switch"
	);
	assert_eq!(widened.location(optional.genesis), optional.ws, "and included with it");
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

	assert_eq!(alone.location(optional.genesis), optional.ws);
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

/// Also checks that a client is handed out only once its chain is synced,
/// which a fast sync would otherwise hide.
#[tokio::test]
#[ignore = "needs network access to the preset's chains"]
async fn the_light_client_reaches_paseo_next_v2() {
	let network =
		NetworkConfig { chain_backend: ChainBackend::LightClient, ..Network::PaseoNextV2.config() };
	CliChainProvider::new(&network)
		.rpc_client(network.asset_hub_genesis)
		.await
		.expect("connect to Asset Hub");
	let light = LightClient::shared(network.id).expect("paseo-next-v2 is bundled");
	let state = light
		.provider
		.lifecycle(network.asset_hub_genesis)
		.expect("Asset Hub is running")
		.next()
		.await;
	assert_eq!(state.map(|state| state.phase), Some(ChainPhase::Ready));

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

	for genesis in [network.people_genesis, network.asset_hub_genesis, network.bulletin_genesis] {
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
