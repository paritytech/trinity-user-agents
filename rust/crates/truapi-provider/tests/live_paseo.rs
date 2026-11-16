//! Live Paseo tests, excluded from CI. Run manually with:
//!
//! ```text
//! cargo test -p truapi-provider --features smoldot -- --ignored
//! ```
//!
//! The light-client test reads the chain-spec path from `PASEO_CHAIN_SPEC`.

#![cfg(all(feature = "ws", not(target_arch = "wasm32")))]

use std::time::Duration;

use futures::stream::StreamExt;
use serde_json::Value;
use truapi_provider::{ChainSource, EmbeddedChainProvider, platform::ChainProvider};

const PASEO_GENESIS: [u8; 32] = [0; 32]; // Registry key only; not validated.
const PASEO_WS_URL: &str = "wss://paseo.rpc.amforc.com";

async fn follow_initializes(source: ChainSource) {
	let provider = EmbeddedChainProvider::builder().chain(PASEO_GENESIS, source).build();
	let connection = provider.connect(PASEO_GENESIS).await.expect("connecting to Paseo succeeds");
	let mut responses = connection.responses();
	connection.send(
		r#"{"jsonrpc":"2.0","id":1,"method":"chainHead_v1_follow","params":[false]}"#.to_owned(),
	);

	let initialized = tokio::time::timeout(Duration::from_secs(300), async {
		loop {
			let frame = responses.next().await.expect("the connection stays alive");
			let frame: Value = serde_json::from_str(&frame).expect("frames are valid JSON");
			if frame["params"]["result"]["event"] == "initialized" {
				return frame;
			}
		}
	})
	.await
	.expect("the follow reaches initialized in time");

	assert!(
		initialized["params"]["result"]["finalizedBlockHashes"]
			.as_array()
			.is_some_and(|hashes| !hashes.is_empty()),
		"initialized must carry finalized hashes"
	);
	connection.close();
}

#[tokio::test]
#[ignore = "requires network access to Paseo"]
async fn ws_follow_initializes() {
	let url = url::Url::parse(PASEO_WS_URL).expect("static URL parses");
	follow_initializes(ChainSource::rpc_node(url)).await;
}

/// Every chain of the bundled `paseo` network syncs on the light client from
/// its genesis hash alone, the way a host connects to it: the follow
/// initializes once the chain is synced, and a later `finalized` event shows
/// the chain keeps up with the network.
#[cfg(feature = "networks")]
#[tokio::test]
#[ignore = "requires network access to Paseo"]
async fn paseo_network_follows_finalized_blocks() {
	let (_, chains) = EmbeddedChainProvider::builder()
		.add_network("paseo")
		.expect("the network is bundled");
	let provider = EmbeddedChainProvider::builder().catalog().build();

	let follows = [chains.relay, chains.assethub, chains.bulletin, chains.people].map(|genesis| {
		let provider = &provider;
		async move {
			let connection =
				provider.connect(genesis).await.expect("the catalog resolves the chain");
			let mut responses = connection.responses();
			connection.send(
				r#"{"jsonrpc":"2.0","id":1,"method":"chainHead_v1_follow","params":[false]}"#
					.to_owned(),
			);
			let event = tokio::time::timeout(Duration::from_secs(300), async {
				let mut initialized = false;
				loop {
					let frame = responses.next().await.expect("the connection stays alive");
					let frame: Value = serde_json::from_str(&frame).expect("frames are valid JSON");
					match frame["params"]["result"]["event"].as_str() {
						Some("initialized") => initialized = true,
						Some("finalized") if initialized => return frame,
						Some("stop") => panic!("the follow of {} stopped", hex::encode(genesis)),
						_ => {},
					}
				}
			})
			.await;
			connection.close();
			event.unwrap_or_else(|_| panic!("{} finalized no block in time", hex::encode(genesis)))
		}
	});
	futures::future::join_all(follows).await;
}

/// The Paseo relay chain spec named by `PASEO_CHAIN_SPEC`.
#[cfg(feature = "smoldot")]
fn paseo_spec() -> String {
	let path = std::env::var("PASEO_CHAIN_SPEC")
		.expect("set PASEO_CHAIN_SPEC to a Paseo relay chain-spec path");
	std::fs::read_to_string(path).expect("the chain spec is readable")
}

#[cfg(feature = "smoldot")]
#[tokio::test]
#[ignore = "requires network access to Paseo and PASEO_CHAIN_SPEC"]
async fn light_follow_initializes() {
	follow_initializes(ChainSource::light_client(paseo_spec()).build()).await;
}

/// Offline, smoldot settles on `Ready` from the checkpoint after its
/// sync-mode deadline, so reaching `Ready` alone proves nothing. A cold start
/// against the network has to reach it with peers.
#[cfg(feature = "smoldot")]
#[tokio::test]
#[ignore = "requires network access to Paseo and PASEO_CHAIN_SPEC"]
async fn light_lifecycle_reaches_ready() {
	use truapi_provider::ChainPhase;

	let provider = EmbeddedChainProvider::builder()
		.chain(PASEO_GENESIS, ChainSource::light_client(paseo_spec()).build())
		.build();
	let connection = provider.connect(PASEO_GENESIS).await.expect("connecting to Paseo succeeds");
	let mut lifecycle =
		provider.lifecycle(PASEO_GENESIS).expect("a connected chain has a lifecycle");

	// A checkpoint close to the finalized head has nothing to warp through,
	// so `Syncing` may be skipped.
	let ready = tokio::time::timeout(Duration::from_secs(300), async {
		loop {
			let state = lifecycle.next().await.expect("the chain stays running");
			match state.phase {
				ChainPhase::Syncing { at, target } => {
					assert!(at <= target, "sync progress {at} passed its target {target}");
				},
				ChainPhase::Ready => return state,
				ChainPhase::Connecting => {},
			}
		}
	})
	.await
	.expect("the chain reaches ready in time");

	assert!(ready.peers > 0, "ready with peers, not stranded offline");
	connection.close();
}
