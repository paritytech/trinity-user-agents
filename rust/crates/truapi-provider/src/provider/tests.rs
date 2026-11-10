use crate::platform::ChainProvider;

use super::EmbeddedChainProvider;
use crate::config::ChainSource;

#[test]
fn unknown_genesis_is_an_error_naming_the_hash() {
	let provider = EmbeddedChainProvider::builder().build();
	let error = futures::executor::block_on(provider.connect([0xab; 32]))
		.err()
		.expect("connect must fail for an unregistered genesis");
	assert!(error.to_string().contains(&"ab".repeat(32)));
}

/// The genesis hashes of the bundled Paseo Next v2 network.
#[cfg(feature = "networks")]
fn paseo_next_v2() -> crate::NetworkChains {
	let (_, chains) = EmbeddedChainProvider::builder()
		.add_network("paseo-next-v2")
		.expect("the network is bundled");
	chains
}

/// An unregistered chain must not start the light client from the catalog,
/// including on a provider with nothing registered.
#[cfg(all(feature = "ws", feature = "networks"))]
#[test]
fn a_provider_without_the_catalog_does_not_resolve_from_it() {
	let catalog = paseo_next_v2();
	let url = url::Url::parse("ws://people.invalid").expect("static URL parses");
	let empty = EmbeddedChainProvider::builder().build();
	let remote = EmbeddedChainProvider::builder()
		.chain(catalog.people, ChainSource::rpc_node(url))
		.build();

	for provider in [empty, remote] {
		let Err(error) = futures::executor::block_on(provider.connect(catalog.relay)) else {
			panic!("an unregistered genesis must not connect");
		};
		assert!(
			matches!(error, crate::ProviderError::UnknownGenesis { genesis } if genesis == catalog.relay),
			"{error}"
		);
		assert!(!provider.is_connected(catalog.relay), "the light client never started");
	}
}

/// A chain `connect` would refuse is refused here too, so no stored blob is
/// read for it.
#[cfg(feature = "networks")]
#[test]
fn load_database_refuses_a_chain_the_provider_cannot_connect_to() {
	let catalog = paseo_next_v2();
	let store = std::sync::Arc::new(MemoryStorageClient::default());
	for genesis in [catalog.bulletin, catalog.relay] {
		store
			.stored
			.lock()
			.expect("test store")
			.insert(genesis, "stored-blob".to_owned());
	}
	let provider = EmbeddedChainProvider::builder().storage(store.clone()).build();

	let Err(error) = futures::executor::block_on(provider.load_database(catalog.bulletin)) else {
		panic!("a chain the provider cannot connect to has nothing to load");
	};
	assert!(
		matches!(error, crate::ProviderError::UnknownGenesis { genesis } if genesis == catalog.bulletin),
		"{error}"
	);
	assert_eq!(store.loads.load(std::sync::atomic::Ordering::Relaxed), 0);
}

/// The native bindings register nothing and reach every chain through the
/// catalog, so a catalog chain they never registered still loads.
#[cfg(feature = "networks")]
#[test]
fn load_database_reads_a_catalog_chain_on_a_provider_with_the_catalog() {
	let catalog = paseo_next_v2();
	let store = std::sync::Arc::new(MemoryStorageClient::default());
	store
		.stored
		.lock()
		.expect("test store")
		.insert(catalog.relay, "stored-blob".to_owned());
	let provider = EmbeddedChainProvider::builder().catalog().storage(store.clone()).build();

	assert!(
		futures::executor::block_on(provider.load_database(catalog.relay))
			.expect("the store answers")
	);
}

/// The parachain with `parachain_spec`, its relay seeded with a blob.
#[cfg(feature = "smoldot")]
fn parachain_with_a_seeded_relay(parachain_spec: &'static str) -> EmbeddedChainProvider {
	use crate::test_specs::{PARACHAIN_GENESIS, RELAY_GENESIS, RELAY_SPEC};

	EmbeddedChainProvider::builder()
		.chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
		.parachain(
			PARACHAIN_GENESIS,
			ChainSource::light_client(parachain_spec).build(),
			RELAY_GENESIS,
		)
		.database(RELAY_GENESIS, "relay-blob".to_owned())
		.build()
}

/// A blob registered for a relay reaches it even when the relay is never
/// connected directly, only brought up behind one of its parachains.
#[cfg(feature = "smoldot")]
#[test]
fn a_relay_blob_seeds_the_relay_brought_up_behind_a_parachain() {
	use crate::test_specs::{PARACHAIN_GENESIS, PARACHAIN_SPEC, RELAY_GENESIS};

	let provider = parachain_with_a_seeded_relay(PARACHAIN_SPEC);

	let _connection = futures::executor::block_on(provider.connect(PARACHAIN_GENESIS))
		.expect("the parachain syncs through its relay");

	assert!(!provider.has_database(RELAY_GENESIS), "the relay took its blob");
}

/// The relay is the chain that warp syncs, so losing its blob to a parachain
/// that failed to start would make the retry sync cold.
#[cfg(feature = "smoldot")]
#[test]
fn a_failed_parachain_connect_keeps_the_relays_blob() {
	use crate::test_specs::{PARACHAIN_GENESIS, RELAY_GENESIS};

	let provider = parachain_with_a_seeded_relay("not a chain spec");

	assert!(futures::executor::block_on(provider.connect(PARACHAIN_GENESIS)).is_err());

	assert!(provider.has_database(RELAY_GENESIS));
}

/// The snapshot round trip resolves against a live light client, and the
/// blob it returns is accepted back as a warm-start seed.
///
/// `chainHead_unstable_finalizedDatabase` first asks the runtime service for
/// the finalized runtime's storage merkle values, and that answer waits on
/// smoldot's sync-mode decision, which no reachable peer will ever settle
/// here. The test platform collapses that deadline; see
/// [`crate::light_platform_test`].
#[cfg(feature = "smoldot")]
#[test]
fn snapshot_round_trips_into_a_seed() {
	const GENESIS: [u8; 32] = [1; 32];
	const SPEC: &str = include_str!("../../tests/fixtures/paseo.json");
	let provider = EmbeddedChainProvider::builder()
		.chain(GENESIS, ChainSource::light_client(SPEC).build())
		.build();
	let blob = futures::executor::block_on(provider.snapshot(GENESIS))
		.expect("the light client answers the finalized-database request");
	assert!(blob.contains("genesisHash"), "unexpected blob: {blob}");

	let seeded = EmbeddedChainProvider::builder()
		.chain(GENESIS, ChainSource::light_client(SPEC).build())
		.database(GENESIS, blob)
		.build();
	futures::executor::block_on(seeded.connect(GENESIS))
		.expect("a chain seeded with its own snapshot connects");
}

/// A store answering from memory, recording what it was asked to keep.
#[cfg(feature = "smoldot")]
#[derive(Default)]
struct MemoryStorageClient {
	stored: std::sync::Mutex<std::collections::HashMap<[u8; 32], String>>,
	loads: std::sync::atomic::AtomicUsize,
}

#[cfg(feature = "smoldot")]
#[async_trait::async_trait]
impl crate::storage::StorageClient for MemoryStorageClient {
	async fn load(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<Option<String>, crate::storage::StorageClientError> {
		self.loads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		Ok(self.stored.lock().expect("test store").get(&genesis_hash).cloned())
	}

	async fn save(
		&self,
		genesis_hash: [u8; 32],
		blob: String,
	) -> Result<(), crate::storage::StorageClientError> {
		self.stored.lock().expect("test store").insert(genesis_hash, blob);
		Ok(())
	}
}

/// A stored blob reaches the chain source the light backend is handed.
#[cfg(feature = "smoldot")]
#[test]
#[allow(irrefutable_let_patterns)]
fn a_stored_blob_seeds_a_chain() {
	const GENESIS: [u8; 32] = [3; 32];
	let store = std::sync::Arc::new(MemoryStorageClient::default());
	store
		.stored
		.lock()
		.expect("test store")
		.insert(GENESIS, "stored-blob".to_owned());

	let provider = EmbeddedChainProvider::builder()
		.chain(GENESIS, ChainSource::light_client("{}").build())
		.storage(store.clone())
		.build();

	assert!(
		futures::executor::block_on(provider.load_database(GENESIS)).expect("the store answers"),
		"a stored blob is reported as in hand"
	);

	futures::executor::block_on(provider.load_database(GENESIS)).expect("the store answers");
	assert_eq!(
		store.loads.load(std::sync::atomic::Ordering::Relaxed),
		1,
		"a blob already in hand is not read again"
	);

	let seeded = provider.with_seeded_database(GENESIS, ChainSource::light_client("{}").build());
	let ChainSource::LightClient { database_content, .. } = seeded else {
		panic!("expected a LightClient source");
	};
	assert_eq!(database_content.as_deref(), Some("stored-blob"));
	assert!(
		!provider.has_database(GENESIS),
		"the blob is taken by the chain that consumed it rather than retained"
	);
}

/// An explicitly registered blob is not replaced by the stored one.
#[cfg(feature = "smoldot")]
#[test]
fn an_explicit_blob_wins_over_the_store() {
	const GENESIS: [u8; 32] = [4; 32];
	let store = std::sync::Arc::new(MemoryStorageClient::default());
	store
		.stored
		.lock()
		.expect("test store")
		.insert(GENESIS, "stored-blob".to_owned());

	let provider = EmbeddedChainProvider::builder()
		.chain(GENESIS, ChainSource::light_client("{}").build())
		.database(GENESIS, "explicit-blob".to_owned())
		.storage(store.clone())
		.build();

	assert!(futures::executor::block_on(provider.load_database(GENESIS)).expect("no store read"));
	assert_eq!(
		store.loads.load(std::sync::atomic::Ordering::Relaxed),
		0,
		"an explicit blob means the store is never consulted"
	);
}

/// Persisting a chain nobody connected would start one just to snapshot it.
#[cfg(feature = "smoldot")]
#[test]
fn persist_without_a_connection_stores_nothing() {
	const GENESIS: [u8; 32] = [7; 32];
	let store = std::sync::Arc::new(MemoryStorageClient::default());
	let provider = EmbeddedChainProvider::builder()
		.chain(GENESIS, ChainSource::light_client("{}").build())
		.storage(store.clone())
		.build();

	assert!(
		!futures::executor::block_on(provider.save_database(GENESIS))
			.expect("no snapshot attempted"),
		"a chain that was never connected has nothing to store"
	);
	assert!(
		store.stored.lock().expect("test store").is_empty(),
		"nothing is written for a chain that was never connected"
	);
}

/// A client implemented in Rust is driven by both database calls, which is
/// the coverage the deleted file store used to give.
#[cfg(feature = "smoldot")]
#[test]
// Without the `ws` backend the enum has a single variant, making the
// let-else below irrefutable there.
#[allow(irrefutable_let_patterns)]
fn a_rust_client_is_driven_by_both_database_calls() {
	const GENESIS: [u8; 32] = [10; 32];
	let client = std::sync::Arc::new(MemoryStorageClient::default());
	client
		.stored
		.lock()
		.expect("test client")
		.insert(GENESIS, "stored-blob".to_owned());

	let provider = EmbeddedChainProvider::builder()
		.chain(GENESIS, ChainSource::light_client("{}").build())
		.storage(client.clone())
		.build();

	assert!(
		futures::executor::block_on(provider.load_database(GENESIS)).expect("the client answers")
	);
	assert_eq!(client.loads.load(std::sync::atomic::Ordering::Relaxed), 1);

	let seeded = provider.with_seeded_database(GENESIS, ChainSource::light_client("{}").build());
	let ChainSource::LightClient { database_content, .. } = seeded else {
		panic!("expected a LightClient source");
	};
	assert_eq!(
		database_content.as_deref(),
		Some("stored-blob"),
		"what the client returned is what the chain is seeded with"
	);
}

/// A snapshot that lost the runtime code must not replace a stored blob
/// that still has it. Flipping the guard in `store_blob` makes this fail,
/// which the unit test on `is_worth_storing` alone does not catch.
#[cfg(feature = "smoldot")]
#[test]
fn a_snapshot_without_the_runtime_code_does_not_replace_one_with_it() {
	const GENESIS: [u8; 32] = [11; 32];
	const WITH_CODE: &str = r#"{"chain":{"a":1},"runtimeCode":"AAAA"}"#;
	const WITHOUT_CODE: &str = r#"{"chain":{"a":1}}"#;

	let client = std::sync::Arc::new(MemoryStorageClient::default());
	client.stored.lock().expect("test client").insert(GENESIS, WITH_CODE.to_owned());

	let provider = EmbeddedChainProvider::builder()
		.chain(GENESIS, ChainSource::light_client("{}").build())
		.storage(client.clone())
		.build();

	assert!(
		!futures::executor::block_on(provider.store_blob(GENESIS, WITHOUT_CODE.to_owned()))
			.expect("the client answers"),
		"a blob that lost the runtime code is not worth storing"
	);
	assert_eq!(
		client.stored.lock().expect("test client").get(&GENESIS),
		Some(&WITH_CODE.to_owned()),
		"the better blob is still there"
	);
}

/// A blob carrying the runtime code always wins, and does so without
/// reading the stored one back.
#[cfg(feature = "smoldot")]
#[test]
fn a_snapshot_with_the_runtime_code_is_stored_without_a_read() {
	const GENESIS: [u8; 32] = [12; 32];
	const WITH_CODE: &str = r#"{"chain":{"a":1},"runtimeCode":"AAAA"}"#;

	let client = std::sync::Arc::new(MemoryStorageClient::default());
	let provider = EmbeddedChainProvider::builder()
		.chain(GENESIS, ChainSource::light_client("{}").build())
		.storage(client.clone())
		.build();

	assert!(
		futures::executor::block_on(provider.store_blob(GENESIS, WITH_CODE.to_owned()))
			.expect("the client accepts it")
	);
	assert_eq!(
		client.loads.load(std::sync::atomic::Ordering::Relaxed),
		0,
		"a blob with the runtime code needs no comparison"
	);
	assert_eq!(
		client.stored.lock().expect("test client").get(&GENESIS),
		Some(&WITH_CODE.to_owned())
	);
}

/// A blob with nothing finalized is never stored, whatever is there.
#[cfg(feature = "smoldot")]
#[test]
fn a_snapshot_with_no_chain_information_is_never_stored() {
	const GENESIS: [u8; 32] = [13; 32];

	let client = std::sync::Arc::new(MemoryStorageClient::default());
	let provider = EmbeddedChainProvider::builder()
		.chain(GENESIS, ChainSource::light_client("{}").build())
		.storage(client.clone())
		.build();

	assert!(
		!futures::executor::block_on(
			provider.store_blob(GENESIS, r#"{"genesisHash":"0x01"}"#.to_owned())
		)
		.expect("the client answers")
	);
	assert!(client.stored.lock().expect("test client").is_empty());
}

/// Once this provider has written, it knows what it left behind and stops
/// reading the blob back on every snapshot.
#[cfg(feature = "smoldot")]
#[test]
fn the_stored_quality_is_remembered_after_the_first_write() {
	const GENESIS: [u8; 32] = [14; 32];
	const WITHOUT_CODE: &str = r#"{"chain":{"a":1}}"#;

	let client = std::sync::Arc::new(MemoryStorageClient::default());
	let provider = EmbeddedChainProvider::builder()
		.chain(GENESIS, ChainSource::light_client("{}").build())
		.storage(client.clone())
		.build();

	for _ in 0..3 {
		assert!(
			futures::executor::block_on(provider.store_blob(GENESIS, WITHOUT_CODE.to_owned()))
				.expect("the client answers")
		);
	}
	assert_eq!(
		client.loads.load(std::sync::atomic::Ordering::Relaxed),
		1,
		"only the first write has to look at what was already there"
	);
}

/// A provider with nowhere to keep blobs says so, rather than answering the
/// same way a working client answers for a chain it has nothing stored for.
#[cfg(feature = "smoldot")]
#[test]
fn load_database_without_storage_is_an_error_naming_what_is_missing() {
	let provider = EmbeddedChainProvider::builder()
		.chain([5; 32], ChainSource::light_client("{}").build())
		.build();
	let error = futures::executor::block_on(provider.load_database([5; 32]))
		.expect_err("a provider with no storage cannot load a database");
	assert!(error.to_string().contains("without storage"), "{error}");
}

#[cfg(feature = "ws")]
#[test]
// On wasm32 without the smoldot backend the enum has a single variant,
// making the let-else irrefutable there.
#[allow(irrefutable_let_patterns)]
fn later_registration_wins() {
	let first = url::Url::parse("ws://first.example").expect("static URL parses");
	let second = url::Url::parse("ws://second.example").expect("static URL parses");
	let provider = EmbeddedChainProvider::builder()
		.chain([1; 32], ChainSource::rpc_node(first))
		.chain([1; 32], ChainSource::rpc_node(second.clone()))
		.build();
	let ChainSource::RpcNode { url } = &provider.chains[&[1; 32]] else {
		panic!("expected an RpcNode source");
	};
	assert_eq!(*url, second);
}

/// A spec that could not be fetched leaves its chain unknown and the rest of
/// the stack running, and the chain comes up on the same provider once the
/// host has the spec.
#[cfg(feature = "smoldot")]
#[test]
fn a_chain_without_its_spec_is_unknown_until_the_spec_is_added() {
	use crate::test_stack::{PEOPLE_SPEC, RELAY_SPEC, STACK_ID, genesis, stack};

	let (relay, people) = (genesis(RELAY_SPEC), genesis(PEOPLE_SPEC));
	let stack = stack(STACK_ID, "one");
	stack.add_light_spec(people, PEOPLE_SPEC.as_bytes()).expect("the spec matches");
	let provider = EmbeddedChainProvider::builder().stack(stack).build();

	let refusals =
		[relay, people].map(|genesis| futures::executor::block_on(provider.connect(genesis)).err());
	assert_eq!(
		refusals,
		[
			Some(crate::ProviderError::UnknownGenesis { genesis: relay }),
			Some(crate::ProviderError::UnknownRelay { relay }),
		]
	);

	provider.add_light_spec(relay, RELAY_SPEC.as_bytes()).expect("the spec matches");
	let _connection = futures::executor::block_on(provider.connect(people))
		.expect("People syncs through the relay the descriptor names");
	assert_eq!(provider.relay_count(), 1, "the relay came up behind People");
	assert_eq!(provider.relay_of(people), Some(relay));
}

/// A spec arriving after the build is checked like one arriving before it.
#[cfg(feature = "smoldot")]
#[test]
fn a_wrong_spec_added_after_the_build_is_refused() {
	use crate::test_stack::{PEOPLE_SPEC, RELAY_SPEC, STACK_ID, genesis, stack};

	let relay = genesis(RELAY_SPEC);
	let provider = EmbeddedChainProvider::builder().stack(stack(STACK_ID, "one")).build();

	let error = provider
		.add_light_spec(relay, PEOPLE_SPEC.as_bytes())
		.expect_err("People's spec is not the relay's");

	assert_eq!(
		error,
		crate::ProviderError::LightSpec {
			role: "Relay".to_owned(),
			genesis: relay,
			source: trinity_stack::Error::SpecDigest,
		}
	);
	assert_eq!(
		EmbeddedChainProvider::builder()
			.build()
			.add_light_spec(relay, RELAY_SPEC.as_bytes()),
		Err(crate::ProviderError::NoStack),
		"a provider without a stack has no descriptor to check a spec against"
	);
}

/// A stack's provider answers only for its descriptor's chains, so a host
/// that asked for one stack never syncs a bundled network instead.
#[cfg(feature = "networks")]
#[test]
fn a_stack_provider_does_not_resolve_from_the_catalog() {
	use crate::test_stack::{STACK_ID, synced_stack};

	let catalog = paseo_next_v2();
	let provider = EmbeddedChainProvider::builder()
		.catalog()
		.stack(synced_stack(STACK_ID, "one"))
		.build();

	let Err(error) = futures::executor::block_on(provider.connect(catalog.people)) else {
		panic!("a chain outside the stack must not connect");
	};
	assert_eq!(error, crate::ProviderError::UnknownGenesis { genesis: catalog.people });
	assert!(provider.relay_of(catalog.people).is_none());
}
