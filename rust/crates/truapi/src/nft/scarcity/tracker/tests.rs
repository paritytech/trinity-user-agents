use std::{
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};

use futures::{
	StreamExt,
	channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
	executor::block_on,
	stream::BoxStream,
};
use subxt::{client::OnlineClientAtBlock, config::SubstrateConfig, utils::H256};
use tempfile::TempDir;

use super::*;
use crate::{
	chain::{DecodableAtBlock, HashAndNumber, Node, RawStorageChanges, StorageSnapshot, serve},
	chain_runtime::{ChainRuntime, RuntimeFailure},
	nft::{
		PurseId, PurseTag, insert_item_row,
		scarcity::testing::{ASSET_HUB_METADATA_BYTES, asset_hub_metadata},
		test_ledger,
	},
	store::{Db, open_temporary_core_db},
	test_support::test_spawner,
	time::{ManualTimer, Timer},
};

const GENESIS: H256 = H256([0xab; 32]);

/// An account holding an NFT on Paseo Asset Hub Next, the owner of the
/// captured value.
const HOLDER_HEX: &str = "fa7c14003d2bb71865069df160fb638e3cd4dac2d6ca394e4fc7fade1bc95644";

/// `Scarcity.NftsByOwner[HOLDER]` as the node names it: the pallet and entry
/// prefix, then the holder under `Blake2_128Concat`.
const HOLDER_STORAGE_KEY_HEX: &str = concat!(
	"9d300807b8c342c90eea46ef234474a18739e9cb7ce404a3933d9260da537988",
	"008f5ac28d4217e2cc6ea0cfcbdfe435",
	"fa7c14003d2bb71865069df160fb638e3cd4dac2d6ca394e4fc7fade1bc95644",
);

/// An item key that holds nothing on the live chain, sorting before `HOLDER`.
const OTHER_KEY: [u8; 32] = [0x01; 32];

fn holder() -> [u8; 32] {
	let bytes = hex::decode(HOLDER_HEX).expect("the holder is hex");
	bytes.try_into().expect("the holder is 32 bytes")
}

/// What `HOLDER` holds: instance 330 of item 13 in collection 27.
const HOLDER_NFT: &[u8] =
	include_bytes!("../../../../tests/fixtures/paseo-next-asset-hub-nfts-by-owner.scale");

const HOLDER_ITEM: HeldItem = HeldItem { collection: 27, item: 13, instance: 330 };

/// How long a test waits for the tracker to write what it was told.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// A watch the tracker opened: the raw keys it asked for, and the channel a
/// test reports their changes on.
struct OpenedWatch {
	keys: Vec<Vec<u8>>,
	changes: UnboundedSender<Result<RawStorageChanges, RuntimeFailure>>,
}

/// A chain whose every watch a test scripts by hand. The client for encoding
/// keys comes from a scripted node serving the Asset Hub runtime.
struct ScriptedChain {
	chain: ChainRuntime,
	opened: Mutex<UnboundedSender<OpenedWatch>>,
}

impl ScriptedChain {
	fn new(opened: UnboundedSender<OpenedWatch>) -> Self {
		let mut node = Node::new(3, 5);
		node.metadata = ASSET_HUB_METADATA_BYTES;
		let (_node, _provider, chain) = serve(node);
		Self { chain, opened: Mutex::new(opened) }
	}
}

#[async_trait::async_trait]
impl WatchedStorage for ScriptedChain {
	async fn client_at_finalized_head(
		&self,
		genesis: H256,
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, RuntimeFailure> {
		self.chain.client_at_finalized_head(genesis).await
	}

	async fn watch_raw_key_changes(
		&self,
		_genesis: H256,
		_head: WatchedHead,
		keys: Vec<Vec<u8>>,
	) -> Result<BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>, RuntimeFailure> {
		let (changes, reported) = unbounded();
		let opened = OpenedWatch { keys, changes };
		self.opened
			.lock()
			.unwrap()
			.unbounded_send(opened)
			.expect("the test listens for watches");
		Ok(reported.boxed())
	}

	async fn read_raw_keys_at(
		&self,
		_genesis: H256,
		_at: H256,
		_keys: Vec<Vec<u8>>,
	) -> Result<StorageSnapshot, RuntimeFailure> {
		Err(RuntimeFailure::host_failure("test", "this fake only watches"))
	}

	fn timer(&self) -> Arc<dyn Timer> {
		Arc::new(ManualTimer::default())
	}
}

/// A running tracker over a fresh ledger, the watches it opens, and the
/// database it writes to.
struct TrackerFixture {
	_dir: TempDir,
	db: Db,
	ledger: NftLedger,
	opened: UnboundedReceiver<OpenedWatch>,
	_tracker: HoldingTracker,
}

/// Starts a tracker after `prepare` has written to the database.
fn tracker_after(prepare: impl FnOnce(&Db)) -> TrackerFixture {
	let (dir, db) = open_temporary_core_db();
	let ledger = test_ledger(db.clone());
	block_on(ledger.create_purse("tickets".to_owned(), PurseTag::default())).expect("purse");
	prepare(&db);
	let (announce, opened) = unbounded();
	let tracker = HoldingTracker::start(HoldingTrackerDeps {
		ledger: ledger.clone(),
		storage: Arc::new(ScriptedChain::new(announce)),
		genesis: GENESIS,
		spawner: test_spawner(),
	});
	TrackerFixture { _dir: dir, db, ledger, opened, _tracker: tracker }
}

/// Waits until `reached` holds, or fails the test.
fn wait_until(what: &str, reached: impl Fn() -> bool) {
	let started = Instant::now();
	while !reached() {
		assert!(started.elapsed() < WRITE_TIMEOUT, "never {what}");
		std::thread::sleep(Duration::from_millis(10));
	}
}

/// Records `key` as an item key of purse 0.
fn insert_item_key(db: &Db, key: [u8; 32], index: u32) {
	insert_item_row(db, ItemKey(key), index);
}

fn block(number: u64) -> HashAndNumber {
	let byte = u8::try_from(number).expect("a test block number fits a byte");
	HashAndNumber { hash: H256::repeat_byte(byte), number }
}

/// The raw changes a node reports at block `number`, by key position.
fn raw_changes_at(number: u64, changed: Vec<(usize, Option<Vec<u8>>)>) -> RawStorageChanges {
	DecodableAtBlock { at: block(number), metadata: asset_hub_metadata(), value: changed }
}

impl TrackerFixture {
	fn next_watch(&mut self) -> OpenedWatch {
		block_on(self.opened.next()).expect("the tracker opens a watch")
	}

	/// Every holding row, by key.
	fn holdings(&self) -> Vec<([u8; 32], HeldItem)> {
		let read = self.db.read(|conn| {
			let mut statement = conn
				.prepare("SELECT key, collection, item, instance FROM nft_holding ORDER BY key")?;
			let rows = statement.query_map([], holding_row)?;
			Ok(rows.collect::<Result<Vec<_>, _>>()?)
		});
		block_on(read).expect("holdings are read")
	}

	/// Waits until the holding rows are `expected`, or fails the test.
	fn await_holdings(&self, expected: Vec<([u8; 32], HeldItem)>) {
		let what = format!("held {expected:?}");
		wait_until(&what, || self.holdings() == expected);
	}
}

fn holding_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<([u8; 32], HeldItem)> {
	let held = HeldItem {
		collection: row.get("collection")?,
		item: row.get("item")?,
		instance: row.get::<_, i64>("instance")? as u64,
	};
	Ok((row.get("key")?, held))
}

impl OpenedWatch {
	/// Reports `changed`, by key position, at block `number`.
	fn report(&self, number: u64, changed: Vec<(usize, Option<Vec<u8>>)>) {
		let changes = raw_changes_at(number, changed);
		self.changes.unbounded_send(Ok(changes)).expect("the tracker listens");
	}
}

/// The key the node is asked for must be the one it stores the value
/// under, and the bytes it answers must decode into the NFT the explorer
/// shows for that account. Both were captured from the live chain.
#[test]
fn a_captured_holding_is_watched_under_its_storage_key_and_cached() {
	let mut fixture = tracker_after(|db| insert_item_key(db, holder(), 0));
	let watch = fixture.next_watch();
	watch.report(5, vec![(0, Some(HOLDER_NFT.to_vec()))]);

	fixture.await_holdings(vec![(holder(), HOLDER_ITEM)]);
	let storage_key = hex::decode(HOLDER_STORAGE_KEY_HEX).expect("the key is hex");
	assert_eq!(watch.keys, vec![storage_key]);
}

/// A key whose NFT was sent on is no longer spendable, so `query_purse`
/// must stop listing it.
#[test]
fn an_nft_moved_away_removes_the_holding() {
	let mut fixture = tracker_after(|db| insert_item_key(db, holder(), 0));
	let watch = fixture.next_watch();
	watch.report(5, vec![(0, Some(HOLDER_NFT.to_vec()))]);
	fixture.await_holdings(vec![(holder(), HOLDER_ITEM)]);

	watch.report(6, vec![(0, None)]);

	fixture.await_holdings(vec![]);
}

/// A failed dispatch bumps the nonce without moving the NFT, so the key
/// keeps its holding. The second key's report rides in the same change, so
/// seeing it written proves the bumped value was written too.
#[test]
fn a_change_that_keeps_the_nft_keeps_the_holding() {
	let mut fixture = tracker_after(|db| {
		insert_item_key(db, holder(), 0);
		insert_item_key(db, OTHER_KEY, 1);
	});
	let watch = fixture.next_watch();
	let mut bumped = HOLDER_NFT.to_vec();
	bumped[32] = 1;
	watch.report(5, vec![(0, Some(HOLDER_NFT.to_vec())), (1, None)]);
	watch.report(6, vec![(0, Some(bumped.clone())), (1, Some(bumped))]);

	fixture.await_holdings(vec![(OTHER_KEY, HOLDER_ITEM), (holder(), HOLDER_ITEM)]);
}

/// Keys allocated after the tracker started must be watched too, and only
/// they, since the keys before them are already watched.
#[test]
fn new_item_keys_are_watched_by_a_watch_of_their_own() {
	let mut fixture = tracker_after(|db| insert_item_key(db, holder(), 0));
	let first = fixture.next_watch();
	let count = core::num::NonZeroU32::new(1).expect("count is positive");

	let allocated = block_on(fixture.ledger.allocate_items(PurseId(0), count)).expect("allocates");
	let second = fixture.next_watch();

	let watched_owners = |watch: &OpenedWatch| -> Vec<Vec<u8>> {
		watch.keys.iter().map(|key| key[key.len() - 32..].to_vec()).collect()
	};
	assert_eq!(watched_owners(&first), vec![holder().to_vec()]);
	assert_eq!(watched_owners(&second), vec![allocated[0].0.to_vec()]);
}

/// The cache survives a restart while the chain moves on, so the first
/// report must overwrite whatever an earlier run left behind.
#[test]
fn the_first_report_replaces_holdings_left_from_an_earlier_run() {
	let mut fixture = tracker_after(|db| {
		insert_item_key(db, holder(), 0);
		block_on(db.write(|tx| {
			tx.execute(
				"INSERT INTO nft_holding VALUES (?1, 1, 1, 1)",
				rusqlite::params![holder()],
			)?;
			Ok(())
		}))
		.expect("stale holding is written");
	});
	let watch = fixture.next_watch();

	watch.report(9, vec![(0, None)]);

	fixture.await_holdings(vec![]);
}
