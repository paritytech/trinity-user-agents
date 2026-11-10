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
fn genesis_hash_of_spec(spec: &str) -> [u8; 32] {
	trinity_stack::light_spec_genesis(spec.as_bytes()).expect("a bundled spec is a light spec")
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
