//! Storage watches against a live Asset Hub. Ignored by default; run with
//! `cargo test -p truapi --lib chain::tests::live_storage_watch -- --ignored --nocapture`.
//!
//! `LIVE_ASSET_HUB_WS` and `LIVE_ASSET_HUB_GENESIS` point the tests at
//! another Asset Hub, for instance previewnet's.

use std::sync::Arc;

use futures::{StreamExt, stream::BoxStream};
use subxt::{ext::scale_value::Value, storage::dynamic, utils::H256};
use truapi_provider::{ChainSource, EmbeddedChainProvider, platform::ChainProvider};

use crate::{
	chain::{DecodedEntryChanges, StorageWatchError, WatchedHead, WatchedStorage},
	chain_runtime::{ChainRuntime, RuntimeChainProvider, RuntimeFailure},
	platform::JsonRpcConnection,
	subscription::Spawner,
};

const ASSET_HUB_WS: &str = "wss://paseo-asset-hub-next-rpc.polkadot.io";
const ASSET_HUB_GENESIS: &str = "4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a";

/// Longest wait for one head's report: a finalized head can take several
/// block times to arrive.
const REPORT_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(120);

/// Accounts watched in the batch test, which is more than any watch the core
/// starts today.
const BATCH_SIZE: usize = 1000;

/// The chain provider hosts plug in, reduced to one WebSocket node.
struct SingleNodeChainProvider(EmbeddedChainProvider);

#[async_trait::async_trait]
impl RuntimeChainProvider for SingleNodeChainProvider {
	async fn connect(
		&self,
		genesis_hash: Vec<u8>,
	) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
		let genesis: [u8; 32] = genesis_hash.try_into().expect("a 32-byte genesis hash");
		let connection = self.0.connect(genesis).await.map_err(|error| {
			RuntimeFailure::unavailable_with_reason("live_connect", format!("{error:?}"))
		})?;
		Ok(Arc::from(connection))
	}
}

fn live_asset_hub_genesis() -> H256 {
	let hex = std::env::var("LIVE_ASSET_HUB_GENESIS").unwrap_or_else(|_| ASSET_HUB_GENESIS.into());
	let bytes = hex::decode(hex.trim_start_matches("0x")).expect("LIVE_ASSET_HUB_GENESIS is hex");
	H256::from_slice(&bytes)
}

/// The chain runtime over the live node, its tasks on the test's runtime.
fn chain_runtime_over_live_asset_hub(genesis: H256) -> Arc<dyn WatchedStorage> {
	let url = std::env::var("LIVE_ASSET_HUB_WS").unwrap_or_else(|_| ASSET_HUB_WS.into());
	let source = ChainSource::rpc_node(url.parse().expect("LIVE_ASSET_HUB_WS is a URL"));
	let provider = EmbeddedChainProvider::builder().chain(genesis.0, source).build();
	let handle = tokio::runtime::Handle::current();
	let spawner: Spawner = Arc::new(move |task| {
		handle.spawn(task);
	});
	Arc::new(ChainRuntime::new(Arc::new(SingleNodeChainProvider(provider)), spawner))
}

/// The next report of `changes`, failing the test on an error or a stall.
async fn next_report_within_timeout<K, V>(
	changes: &mut BoxStream<'static, Result<DecodedEntryChanges<K, V>, StorageWatchError>>,
) -> DecodedEntryChanges<K, V> {
	let next = tokio::time::timeout(REPORT_TIMEOUT, changes.next()).await;
	let report = next.expect("a report before the timeout").expect("the watch goes on");
	report.expect("a successful read")
}

async fn first_three_timestamps_at(head: WatchedHead) -> Vec<(u64, u64)> {
	let genesis = live_asset_hub_genesis();
	let address = dynamic::<(), u64>("Timestamp", "Now");
	let watch = chain_runtime_over_live_asset_hub(genesis).watch_entry_changes(
		genesis,
		head,
		address,
		vec![()],
	);
	let mut changes = watch.await.expect("the watch starts");
	let mut timestamps = Vec::new();
	for _ in 0..3 {
		let report = next_report_within_timeout(&mut changes).await;
		let now = report.changed[0].1.expect("Timestamp.Now has a default");
		timestamps.push((report.at.number, now));
	}
	timestamps
}

/// Each report is a later block with a later timestamp, so the watch follows
/// the chain and reports each new value.
fn assert_blocks_and_timestamps_increase(timestamps: &[(u64, u64)]) {
	for pair in timestamps.windows(2) {
		assert!(pair[1].0 > pair[0].0 && pair[1].1 > pair[0].1, "{timestamps:?}");
	}
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs network access to a live Asset Hub"]
async fn a_best_watch_reports_each_new_timestamp() {
	let timestamps = first_three_timestamps_at(WatchedHead::Best).await;
	println!("best (block, timestamp): {timestamps:?}");

	assert_blocks_and_timestamps_increase(&timestamps);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs network access to a live Asset Hub"]
async fn a_finalized_watch_reports_each_new_timestamp() {
	let timestamps = first_three_timestamps_at(WatchedHead::Finalized).await;
	println!("finalized (block, timestamp): {timestamps:?}");

	assert_blocks_and_timestamps_increase(&timestamps);
}

/// A distinct account nobody holds the key to: `index` followed by `0xee`.
fn distinct_unused_account(index: usize) -> ([u8; 32],) {
	let mut account = [0xee; 32];
	let index = u32::try_from(index).expect("a batch index fits u32");
	account[..4].copy_from_slice(&index.to_le_bytes());
	(account,)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs network access to a live Asset Hub"]
async fn a_thousand_keys_are_answered_in_one_read() {
	// Fresh accounts are all absent, so every one decodes to the default
	// `AccountInfo`; the point is that the node answers every key at once.
	let genesis = live_asset_hub_genesis();
	let accounts: Vec<([u8; 32],)> = (0..BATCH_SIZE).map(distinct_unused_account).collect();
	let address = dynamic::<([u8; 32],), Value>("System", "Account");
	let watch = chain_runtime_over_live_asset_hub(genesis).watch_entry_changes(
		genesis,
		WatchedHead::Finalized,
		address,
		accounts,
	);
	let mut changes = watch.await.expect("the watch starts");

	let started = std::time::Instant::now();
	let report = next_report_within_timeout(&mut changes).await;
	println!("{} keys read in {:?}", report.changed.len(), started.elapsed());

	assert_eq!(report.changed.len(), BATCH_SIZE);
	assert!(report.changed.iter().all(|(_, value)| value.is_some()));
}
