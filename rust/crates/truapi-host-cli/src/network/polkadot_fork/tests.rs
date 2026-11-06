use std::sync::{Arc, Mutex};

use clap::ValueEnum;

use super::{
	super::{Network, testing::serve_rpc},
	*,
};
use crate::chain::ChainBackend;

const POLKADOT_PEOPLE_GENESIS: &str =
	"67fa177a097bfa18f77ea95ab56e9bcdfeb0e5b8a40e46298bb93e16b6fc5008";

/// A loopback node whose finalized block is `finalized` and whose block at
/// each height up to it hashes to `chain` followed by the height, so two
/// nodes with the same `chain` serve the same chain. Records the height of
/// each `chain_getBlockHash` call.
async fn serve_node(chain: u8, finalized: u32) -> (String, Arc<Mutex<Vec<u32>>>) {
	let heights = Arc::new(Mutex::new(Vec::new()));
	let recorded = Arc::clone(&heights);
	let hash = move |height: u32| format!("0x{chain:02x}{height:062x}");
	let url = serve_rpc(move |method, params| match method {
		"chain_getFinalizedHead" => json!(hash(finalized)),
		"chain_getHeader" => json!({ "number": format!("0x{finalized:x}") }),
		"chain_getBlockHash" => {
			let height = params[0].as_u64().expect("no height") as u32;
			recorded.lock().unwrap().push(height);
			if height <= finalized { json!(hash(height)) } else { json!(null) }
		},
		method => panic!("unexpected {method:?}"),
	})
	.await;
	(url, heights)
}

fn polkadot_chain(ws: &str, polkadot_rpc: &[&str]) -> PolkadotChain {
	PolkadotChain {
		endpoint: ChainEndpoint { genesis: [0x67; 32], ws: ws.to_owned() },
		polkadot_rpc: polkadot_rpc.iter().map(|node| (*node).to_owned()).collect(),
	}
}

/// Only an endpoint with one of real Polkadot's genesis hashes, from the
/// bundled reference, is compared, against that chain's nodes; SPEC.md lists
/// the role chains' genesis hashes. Compiled in with `include_str!` so a moved
/// table breaks the build rather than drifting quietly.
#[test]
fn test_polkadot_chains_from_reference() {
	const SPEC: &str = include_str!("../../../SPEC.md");

	for network in Network::value_variants() {
		assert!(polkadot_chains(&network.bundled_stack(ChainBackend::Rpc).chains()).is_empty());
	}
	let mut chains = Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc).chains();
	hex::decode_to_slice(POLKADOT_PEOPLE_GENESIS, &mut chains.people.genesis).unwrap();
	let reference = polkadot_reference();
	let people = reference.chain("People").expect("reference without People");
	assert_eq!(
		polkadot_chains(&chains),
		[PolkadotChain { endpoint: chains.people.clone(), polkadot_rpc: people.rpc.clone() }]
	);

	for (row, genesis) in [
		("People genesis", POLKADOT_PEOPLE_GENESIS),
		("Asset Hub genesis", "68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f"),
		("Bulletin genesis", "2761c95259d59e55ae3daf756c1413b46e45a5a2987299f8ef8e5d8e4776cbc4"),
	] {
		assert!(
			reference
				.chains
				.iter()
				.any(|chain| chain.genesis_hash == format!("0x{genesis}"))
		);
		assert!(SPEC.contains(&format!("| {row} | `0x{genesis}` |")));
	}
}

/// An endpoint whose block at the lower finalized height is real Polkadot's
/// is Polkadot itself, whether it is behind or ahead of the Polkadot node. A
/// Polkadot node that is down leaves the next one to compare against.
#[tokio::test]
async fn test_polkadot_itself_error() {
	for (endpoint_finalized, polkadot_finalized) in [(500, 900), (900, 500)] {
		let (endpoint, _) = serve_node(1, endpoint_finalized).await;
		let (polkadot, _) = serve_node(1, polkadot_finalized).await;
		let error = polkadot_chain(&endpoint, &["ws://127.0.0.1:1", &polkadot])
			.ensure_fork()
			.await
			.expect_err("Polkadot itself accepted");
		assert!(error.to_string().contains("serves Polkadot itself: its block 500"));
	}
}

/// A fork's chain diverges from Polkadot's, so it runs. Both nodes are
/// compared at the lower of their finalized blocks, which neither side can
/// drop, so a fork ahead of Polkadot runs too.
#[tokio::test]
async fn test_finalized_height() {
	for (endpoint_finalized, polkadot_finalized) in [(500, 900), (900, 500)] {
		let (endpoint, endpoint_heights) = serve_node(2, endpoint_finalized).await;
		let (polkadot, polkadot_heights) = serve_node(1, polkadot_finalized).await;
		polkadot_chain(&endpoint, &[&polkadot])
			.ensure_fork()
			.await
			.expect("fork refused");
		assert_eq!(*endpoint_heights.lock().unwrap(), [500]);
		assert_eq!(*polkadot_heights.lock().unwrap(), [500]);
	}
}

/// The CLI cannot show an endpoint is not Polkadot when it, or every Polkadot
/// node, does not answer, so the endpoint is an error.
#[tokio::test]
async fn test_unanswered_error() {
	let (endpoint, _) = serve_node(2, 500).await;
	let (polkadot, _) = serve_node(1, 900).await;
	let closed = "ws://127.0.0.1:1";
	for chain in [
		polkadot_chain(closed, &[&polkadot]),
		polkadot_chain(&endpoint, &[closed, "ws://127.0.0.1:2"]),
	] {
		let error = chain.ensure_fork().await.expect_err("unanswered check accepted");
		assert!(error.to_string().contains("cannot tell"));
	}
}
