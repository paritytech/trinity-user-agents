use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use super::*;
use crate::network::Network;

/// The default network with People at `people_ws`.
fn build_network_with_people_at(people_ws: &str) -> Stack {
	Network::default()
		.bundled_stack(ChainBackend::Rpc)
		.with_chain_edit("People", |chain| chain["rpc"] = serde_json::json!([people_ws]))
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
	let network = build_network_with_people_at(&serve_one_frame_per_connection().await);
	let provider = CliChainProvider::new(&network);

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
	let network = build_network_with_people_at(&serve_one_frame_per_connection().await);
	let (_, catalog) = truapi_provider::EmbeddedChainProviderBuilder::new()
		.add_network(network.namespace())
		.expect("the default network is bundled");
	let provider = CliChainProvider::new(&network);

	let connection = provider.connect(catalog.relay).await.expect("connect to People");
	let first =
		tokio::time::timeout(std::time::Duration::from_secs(5), connection.responses().next())
			.await
			.expect("the loopback server answers at once");
	assert_eq!(first.as_deref(), Some("open"));
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
	let network = Network::default().bundled_stack(ChainBackend::LightClient);
	let provider = CliChainProvider::new(&network);

	drop(provider.connect(network.genesis().people).await.expect("add People"));

	let light = light_client(network.namespace()).expect("the default network is bundled");
	assert!(light.is_connected(network.genesis().people));
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
	let network = Network::PaseoNextV2.bundled_stack(ChainBackend::LightClient);
	CliChainProvider::new(&network)
		.rpc_client(network.genesis().asset_hub)
		.await
		.expect("connect to Asset Hub");
	let light = light_client(network.namespace()).expect("paseo-next-v2 is bundled");
	let state = light
		.lifecycle(network.genesis().asset_hub)
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

	let network = Network::PaseoNextV2.bundled_stack(backend);
	let provider = Arc::new(CliChainProvider::new(&network));
	let reported_genesis = async |genesis: [u8; 32]| -> String {
		let rpc = provider.rpc_client(genesis).await.expect("connect");
		rpc.request("chainSpec_v1_genesisHash", rpc_params![])
			.await
			.expect("genesis hash")
	};

	for genesis in
		[network.genesis().people, network.genesis().asset_hub, network.genesis().bulletin]
	{
		assert_eq!(reported_genesis(genesis).await, format!("0x{}", hex::encode(genesis)));
	}
	provider
		.connect(network.genesis().people)
		.await
		.expect("connect to People")
		.close();
	assert_eq!(
		reported_genesis(network.genesis().people).await,
		format!("0x{}", hex::encode(network.genesis().people)),
		"a new People connection replaces the closed one"
	);

	let mut reader = AssetHubReader::connect(&network).await.expect("connect to Asset Hub");
	assert!(reader.timestamp_secs().await.expect("Asset Hub time") > 0);
	assert!(reader.dotns_identity(&[0x11; 32]).await.expect("dotNS read").is_none());

	// A light client may have no Bulletin peers yet, so a transient failure is
	// asked again, as a preimage lookup does.
	let bitswap = BitswapRpc::new(Arc::clone(&provider), network.genesis().bulletin);
	let unstored = truapi::BulletinCid::from([0x22; 32]);
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

#[test]
fn first_hop_response_stream_receives_frames_buffered_during_setup() {
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
	assert_eq!(
		futures::executor::block_on(responses.next()).as_deref(),
		Some(r#"{"jsonrpc":"2.0","id":1,"result":"ready"}"#),
	);
}

#[test]
fn closing_hop_ends_responses_and_releases_capacity() {
	use futures::FutureExt;

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
	assert_eq!(responses.next().now_or_never(), Some(None));
	connection.send("must not be sent after close".to_string());
	assert!(outbound_rx.try_recv().is_err());
}

#[tokio::test]
async fn hop_response_stream_ends_when_its_socket_closes() {
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let url = format!("ws://{}", listener.local_addr().unwrap());
	let server = tokio::spawn(async move {
		let (socket, _) = listener.accept().await.unwrap();
		let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
		socket.close(None).await.unwrap();
	});
	let connection = WsJsonRpcConnection::connect(&url, None).await.unwrap();
	let mut responses = connection.responses();
	server.await.unwrap();
	assert_eq!(
		tokio::time::timeout(std::time::Duration::from_secs(5), responses.next())
			.await
			.expect("closing the socket ends its response stream"),
		None,
	);
}

#[tokio::test]
async fn hop_authority_and_connection_limits_are_independent_of_chain_backend() {
	for backend in [ChainBackend::Rpc, ChainBackend::LightClient] {
		let network = Network::PaseoNextV2.bundled_stack(backend);
		let provider = CliChainProvider::new(&network);
		let genesis = network.genesis().bulletin;
		let allowed = provider.allowed_hop_endpoints(genesis).await.unwrap();
		assert_eq!(
			allowed,
			["wss://paseo-hop-next-0.polkadot.io", "wss://paseo-hop-next-1.polkadot.io"],
		);
		assert!(
			provider
				.allowed_hop_endpoints(network.genesis().people)
				.await
				.unwrap()
				.is_empty()
		);
		assert!(
			provider
				.connect_hop(network.genesis().people, allowed[0].clone())
				.await
				.is_err()
		);
		assert!(
			provider
				.connect_hop(genesis, network.chains().bulletin.ws.clone())
				.await
				.is_err()
		);
		assert!(provider.connect_hop(genesis, format!("{}/", allowed[0])).await.is_err());

		let _permits: Vec<_> = (0..MAX_HOP_CONNECTIONS)
			.map(|_| provider.hop_connections.clone().try_acquire_owned().unwrap())
			.collect();
		let error = match provider.connect_hop(genesis, allowed[0].clone()).await {
			Ok(_) => panic!("HOP connection limit must be enforced"),
			Err(error) => error,
		};
		assert_eq!(error.reason, "HOP connection limit reached");
	}
}
