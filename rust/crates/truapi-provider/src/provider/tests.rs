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

/// A blob registered for a relay reaches it even when the relay is never
/// connected directly, only brought up behind one of its parachains.
#[cfg(feature = "smoldot")]
#[test]
// Without the `ws` backend the enum has a single variant, making the
// let-else below irrefutable there.
fn a_relay_blob_seeds_the_relay_brought_up_behind_a_parachain() {
	use std::collections::HashMap;

	const RELAY: [u8; 32] = [1; 32];
	let mut chains = HashMap::new();
	chains.insert(RELAY, ChainSource::light_client("{}").build());
	let provider = EmbeddedChainProvider::builder()
		.database(RELAY, "relay-blob".to_owned())
		.build();

	let (genesis, source) =
		provider.resolve_relay(&chains, RELAY).expect("the relay is registered");
	assert_eq!(genesis, RELAY);
	let database_content = match source {
		ChainSource::LightClient { database_content, .. } => database_content,
		#[cfg(feature = "ws")]
		ChainSource::RpcNode { .. } => panic!("expected a LightClient source"),
	};
	assert_eq!(database_content.as_deref(), Some("relay-blob"));
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
	let database_content = match seeded {
		ChainSource::LightClient { database_content, .. } => database_content,
		#[cfg(feature = "ws")]
		ChainSource::RpcNode { .. } => panic!("expected a LightClient source"),
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
	let database_content = match seeded {
		ChainSource::LightClient { database_content, .. } => database_content,
		#[cfg(feature = "ws")]
		ChainSource::RpcNode { .. } => panic!("expected a LightClient source"),
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
