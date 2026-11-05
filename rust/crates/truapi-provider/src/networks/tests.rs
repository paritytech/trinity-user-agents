/// `ChainCloseReason::StreamEnded` documents that every connection this crate
/// hands a host runs on the embedded light client, whose stream ends only when
/// the connection is closed. That holds because the FFI registers no chains of
/// its own, so every source it resolves comes from this catalog. The `ws`
/// backend ends its stream when a socket drops, which would make that doc
/// false, so a catalog entry must never be one.
#[test]
fn every_catalog_source_is_light_backed() {
	for network in super::CATALOG.iter() {
		let (_, sources) = super::network_sources(network).expect("bundled network resolves");
		assert!(
			!sources.is_empty(),
			"{} contributed no sources, so this test would pass vacuously",
			network.name
		);
		for (genesis, source) in sources {
			assert!(
				matches!(source, crate::ChainSource::LightClient { .. }),
				"{} chain {} is not light-backed, which breaks the StreamEnded contract",
				network.name,
				hex::encode(genesis)
			);
		}
	}
}

use super::*;

#[test]
fn paseo_next_v2_registers_four_chains_with_expected_hashes() {
	let (_, chains) = EmbeddedChainProviderBuilder::new()
		.add_network("paseo-next-v2")
		.expect("bundled network registers");
	assert_eq!(hex::encode(chains.relay), &CATALOG[0].relay.genesis_hex[2..]);
	assert_eq!(hex::encode(chains.people), &CATALOG[0].people.genesis_hex[2..]);
}

/// Every catalog entry's genesis hash must be the hash of the genesis block
/// its own bundled spec describes.
///
/// A spec refreshed against a chain that has since been re-genesised keeps
/// working for the light client (smoldot derives the real hash from the
/// spec), so the stale entry is invisible until a caller connects by the
/// chain's true genesis hash and is told it is unknown. This derives the
/// hash offline and catches that drift at the refresh.
#[test]
fn every_catalog_genesis_hash_matches_its_bundled_spec() {
	for network in CATALOG {
		for (service, chain) in [
			("relay", &network.relay),
			("asset-hub", &network.assethub),
			("bulletin", &network.bulletin),
			("people", &network.people),
		] {
			let derived = genesis_hash_of_spec(chain.spec);
			assert_eq!(
				format!("0x{}", hex::encode(derived)),
				chain.genesis_hex,
				"the {} {} spec describes a different chain than its catalog entry claims; \
                     set its genesis_hex to the left-hand hash",
				network.name,
				service,
			);
		}
	}
}

/// Hash of the genesis block a light chain spec describes.
///
/// A light spec carries only the genesis state root, which is enough: the
/// genesis block has no parent, number zero, no extrinsics and no digest,
/// so its header is fully determined by that root.
fn genesis_hash_of_spec(spec: &str) -> [u8; 32] {
	/// Merkle value of an empty trie, the genesis block's extrinsics root.
	const EMPTY_TRIE_ROOT: &str =
		"03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314";

	let spec: serde_json::Value = serde_json::from_str(spec).expect("bundled spec is JSON");
	let state_root = spec["genesis"]["stateRootHash"]
		.as_str()
		.expect("bundled spec carries a genesis state root");
	let state_root =
		hex::decode(state_root.trim_start_matches("0x")).expect("the state root is hex");

	let mut header = Vec::with_capacity(98);
	header.extend_from_slice(&[0u8; 32]); // parent hash
	header.push(0); // block number, SCALE compact 0
	header.extend_from_slice(&state_root);
	header.extend_from_slice(&hex::decode(EMPTY_TRIE_ROOT).expect("the constant is hex"));
	header.push(0); // digest, an empty vector

	blake2b_simd::Params::new()
		.hash_length(32)
		.hash(&header)
		.as_bytes()
		.try_into()
		.expect("a 32-byte hash")
}

#[test]
fn unknown_network_lists_the_catalog() {
	let error = EmbeddedChainProviderBuilder::new()
		.add_network("mainnet")
		.expect_err("an unbundled network must fail");
	assert!(error.to_string().contains("paseo-next-v2"));
}

#[test]
fn connect_resolves_network_from_genesis_alone() {
	use crate::platform::ChainProvider;
	use futures::{executor::block_on, stream::StreamExt};

	// A provider built with the catalog and nothing registered connects to
	// a catalog chain from its genesis hash alone.
	let relay = genesis(&CATALOG[0].relay).expect("catalog genesis parses");
	let provider = crate::EmbeddedChainProvider::builder().catalog().build();
	let connection = block_on(provider.connect(relay))
		.expect("catalog resolves the relay genesis without registration");
	let mut responses = connection.responses();
	connection.send(
		r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
	);
	let response = block_on(responses.next()).expect("smoldot answers spec-local queries");
	assert!(response.contains("\"Paseo\""), "unexpected response: {response}");
}
