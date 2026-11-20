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
	let relay = crate::test_stack::genesis(include_str!("../../networks/paseo.json"));
	let people =
		crate::test_stack::genesis(include_str!("../../networks/paseo-next-v2-people.json"));
	assert_eq!((chains.relay, chains.people), (relay, people));
}

/// The public Paseo chains do not restart and have no stack descriptor to
/// check a refreshed spec against, so a refresh that derives any other hash
/// pulled its spec from a different chain.
#[test]
fn paseo_chains_keep_their_genesis() {
	let (_, chains) = EmbeddedChainProviderBuilder::new()
		.add_network("paseo")
		.expect("bundled network registers");
	let pinned = [
		"374057be67b355151f271ff70c3db98308c62c8adc48dc6724b6a009a1a014fd",
		"d6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
		"e101f0fa4627d29a257645e02be86d80378fea1a2bf8fa6a918d150ebc760a59",
		"e6c30d6e148f250b887105237bcaa5cb9f16dd203bf7b5b9d4f1da7387cb86ec",
	];
	assert_eq!(
		[chains.relay, chains.assethub, chains.bulletin, chains.people].map(hex::encode),
		pinned
	);
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

/// The bundled network named `name`.
fn network(name: &str) -> &'static NetworkDef {
	CATALOG
		.iter()
		.find(|network| network.name == name)
		.expect("the network is bundled")
}

/// Each chain of `network` with the service name the catalog logs it under.
fn services(network: &NetworkDef) -> [(&'static str, &ChainDef); 4] {
	[
		("relay", &network.relay),
		("asset-hub", &network.assethub),
		("bulletin", &network.bulletin),
		("people", &network.people),
	]
}

/// Sorted genesis hashes of every chain of `network`.
fn chain_hashes(network: &NetworkDef) -> Vec<[u8; 32]> {
	let mut hashes: Vec<_> = services(network)
		.into_iter()
		.map(|(_, chain)| genesis(chain).expect("catalog genesis parses"))
		.collect();
	hashes.sort();
	hashes
}

/// What looking a genesis hash up in the catalog yields.
#[derive(Debug, PartialEq)]
struct Lookup {
	/// The network and service its logs are attributed to.
	service: Option<(&'static str, &'static str)>,
	/// Sorted genesis hashes of the chains it is resolved with.
	chains: Vec<[u8; 32]>,
	/// The relay it syncs through.
	relay: Option<[u8; 32]>,
}

fn lookup(genesis_hash: [u8; 32]) -> Lookup {
	let (chains, relay) = catalog_network_chains(genesis_hash).expect("a catalog chain resolves");
	let mut chains: Vec<_> = chains.into_keys().collect();
	chains.sort();
	Lookup { service: catalog_service(genesis_hash), chains, relay }
}

/// A host connects to a `paseo` parachain by its genesis hash alone, so that
/// hash has to bring up the `paseo` network's chains and sync through the
/// Paseo relay. A parachain resolved with another network's chains would be
/// wired to the wrong siblings.
#[test]
fn paseo_parachains_resolve_to_paseo_through_the_paseo_relay() {
	let paseo = network("paseo");
	let relay = genesis(&PASEO_RELAY).expect("catalog genesis parses");
	for (service, chain) in
		[("asset-hub", &paseo.assethub), ("bulletin", &paseo.bulletin), ("people", &paseo.people)]
	{
		assert_eq!(
			lookup(genesis(chain).expect("catalog genesis parses")),
			Lookup {
				service: Some(("paseo", service)),
				chains: chain_hashes(paseo),
				relay: Some(relay),
			},
			"the paseo {service} chain",
		);
	}
}

/// `paseo` shares its relay with `paseo-next-v2`, which was bundled first.
/// Every `paseo-next-v2` lookup, the shared relay included, has to keep
/// resolving to `paseo-next-v2`.
#[test]
fn paseo_next_v2_lookups_resolve_to_paseo_next_v2() {
	let next = network("paseo-next-v2");
	let relay = genesis(&next.relay).expect("catalog genesis parses");
	for (service, chain) in services(next) {
		let genesis_hash = genesis(chain).expect("catalog genesis parses");
		let syncs_through = (genesis_hash != relay).then_some(relay);
		assert_eq!(
			lookup(genesis_hash),
			Lookup {
				service: Some(("paseo-next-v2", service)),
				chains: chain_hashes(next),
				relay: syncs_through,
			},
			"the paseo-next-v2 {service} chain",
		);
	}
}

/// A lookup returns the first network that lists a genesis hash, so a chain
/// listed twice with two definitions would silently run on the first one only.
#[test]
fn a_chain_listed_by_several_networks_has_one_definition() {
	let chains: Vec<_> = CATALOG
		.iter()
		.flat_map(|network| {
			services(network).map(|(service, chain)| (network.name, service, chain))
		})
		.collect();
	for (index, (network, service, chain)) in chains.iter().enumerate() {
		for (other_network, other_service, other) in &chains[index + 1..] {
			if genesis(chain).expect("invalid bundled spec") ==
				genesis(other).expect("invalid bundled spec")
			{
				assert_eq!(
					(chain.spec, chain.statement_protocol),
					(other.spec, other.statement_protocol),
					"{network} {service} and {other_network} {other_service} share a genesis",
				);
			}
		}
	}
}
