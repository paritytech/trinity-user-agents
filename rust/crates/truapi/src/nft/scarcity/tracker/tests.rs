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
use parity_scale_codec::Decode;
use subxt::{ArcMetadata, utils::H256};
use tempfile::TempDir;

use super::*;
use crate::{
	chain::{DecodableAtBlock, HashAndNumber, RawStorageChanges},
	chain_runtime::RuntimeFailure,
	nft::{PurseId, PurseTag, test_ledger},
	store::{Db, open_temporary_core_db},
	test_support::test_spawner,
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

fn asset_hub_metadata() -> ArcMetadata {
	let bytes =
		include_bytes!("../../../../tests/fixtures/paseo-next-asset-hub-3004000-metadata.scale");
	let prefixed = frame_metadata::RuntimeMetadataPrefixed::decode(&mut &bytes[..])
		.expect("the Asset Hub fixture decodes");
	ArcMetadata::from(subxt::Metadata::try_from(prefixed).expect("the fixture is subxt metadata"))
}

/// A watch the tracker opened: the raw keys it asked for, and the channel a
/// test reports their changes on.
struct OpenedWatch {
	keys: Vec<Vec<u8>>,
	changes: UnboundedSender<Result<RawStorageChanges, RuntimeFailure>>,
}

/// A chain whose every watch a test scripts by hand.
struct ScriptedChain {
	opened: Mutex<UnboundedSender<OpenedWatch>>,
}

#[async_trait::async_trait]
impl WatchedStorage for ScriptedChain {
	async fn metadata_at_finalized_head(
		&self,
		_genesis: H256,
	) -> Result<ArcMetadata, RuntimeFailure> {
		Ok(asset_hub_metadata())
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
}

/// A running tracker over a fresh ledger, the watches it opens, and the
/// database it writes to.
struct TrackerFixture {
	_dir: TempDir,
	db: Db,
	ledger: NftLedger,
	opened: UnboundedReceiver<OpenedWatch>,
	_tracker: Arc<HoldingTracker>,
}

/// Starts a tracker after `prepare` has written to the database.
fn tracker_after(prepare: impl FnOnce(&Db)) -> TrackerFixture {
	let (dir, db) = open_temporary_core_db();
	let ledger = test_ledger(db.clone());
	block_on(ledger.create_purse("tickets".to_owned(), PurseTag::default())).expect("purse");
	prepare(&db);
	let (announce, opened) = unbounded();
	let tracker = HoldingTracker::new(HoldingTrackerDeps {
		ledger: ledger.clone(),
		storage: Arc::new(ScriptedChain { opened: Mutex::new(announce) }),
		genesis: GENESIS,
		spawner: test_spawner(),
	});
	tracker.ensure_started();
	TrackerFixture { _dir: dir, db, ledger, opened, _tracker: tracker }
}

/// Records `key` as an item key of purse 0.
fn insert_item_key(db: &Db, key: [u8; 32], index: u32) {
	block_on(db.write(move |tx| {
		tx.execute(
			"INSERT INTO nft_item (key, purse, installation, idx) VALUES (?1, 0, ?2, ?3)",
			rusqlite::params![key, [0x1du8; 32], index],
		)?;
		Ok(())
	}))
	.expect("item key is written");
}

fn block(number: u64) -> HashAndNumber {
	HashAndNumber { hash: H256::repeat_byte(u8::try_from(number).unwrap()), number }
}

/// The raw changes a node reports at block `number`, by key position.
fn raw_changes_at(number: u64, changed: Vec<(usize, Option<Vec<u8>>)>) -> RawStorageChanges {
	DecodableAtBlock { at: block(number), metadata: asset_hub_metadata(), value: changed }
}

impl TrackerFixture {
	fn next_watch(&mut self) -> OpenedWatch {
		block_on(self.opened.next()).expect("the tracker opens a watch")
	}

	/// Every holding row: key, item and the block it was seen at.
	fn holdings(&self) -> Vec<([u8; 32], HeldItem, u64)> {
		let read = self.db.read(|conn| {
			let mut statement = conn.prepare(
				"SELECT key, collection, item, instance, seen_number FROM nft_holding ORDER BY key",
			)?;
			let rows = statement.query_map([], |row| {
				let held = HeldItem {
					collection: row.get("collection")?,
					item: row.get("item")?,
					instance: row.get::<_, i64>("instance")? as u64,
				};
				Ok((row.get("key")?, held, row.get("seen_number")?))
			})?;
			Ok(rows.collect::<Result<Vec<_>, _>>()?)
		});
		block_on(read).expect("holdings are read")
	}

	/// Waits until the holding rows are `expected`, or fails the test.
	fn await_holdings(&self, expected: Vec<([u8; 32], HeldItem, u64)>) {
		let started = Instant::now();
		while self.holdings() != expected {
			assert!(started.elapsed() < WRITE_TIMEOUT, "holdings stayed {:?}", self.holdings());
			std::thread::sleep(Duration::from_millis(10));
		}
	}
}

/// The key the node is asked for must be the one it stores the value
/// under, and the bytes it answers must decode into the NFT the explorer
/// shows for that account. Both were captured from the live chain.
#[test]
fn a_captured_holding_is_watched_under_its_storage_key_and_cached() {
	let mut fixture = tracker_after(|db| insert_item_key(db, holder(), 0));
	let watch = fixture.next_watch();
	watch
		.changes
		.unbounded_send(Ok(raw_changes_at(5, vec![(0, Some(HOLDER_NFT.to_vec()))])))
		.unwrap();

	fixture.await_holdings(vec![(holder(), HOLDER_ITEM, 5)]);
	let storage_key = hex::decode(HOLDER_STORAGE_KEY_HEX).expect("the key is hex");
	assert_eq!(watch.keys, vec![storage_key]);
}

#[test]
fn an_nft_moved_away_removes_the_holding() {
	let mut fixture = tracker_after(|db| insert_item_key(db, holder(), 0));
	let watch = fixture.next_watch();
	watch
		.changes
		.unbounded_send(Ok(raw_changes_at(5, vec![(0, Some(HOLDER_NFT.to_vec()))])))
		.unwrap();
	fixture.await_holdings(vec![(holder(), HOLDER_ITEM, 5)]);

	watch.changes.unbounded_send(Ok(raw_changes_at(6, vec![(0, None)]))).unwrap();

	fixture.await_holdings(vec![]);
}

/// A failed dispatch bumps the nonce without moving the NFT, so the
/// holding stays and only the block it was seen at moves on.
#[test]
fn a_change_that_keeps_the_nft_only_moves_the_block_it_was_seen_at() {
	let mut fixture = tracker_after(|db| insert_item_key(db, holder(), 0));
	let watch = fixture.next_watch();
	let mut bumped = HOLDER_NFT.to_vec();
	bumped[32] = 1;
	watch
		.changes
		.unbounded_send(Ok(raw_changes_at(5, vec![(0, Some(HOLDER_NFT.to_vec()))])))
		.unwrap();
	watch
		.changes
		.unbounded_send(Ok(raw_changes_at(6, vec![(0, Some(bumped))])))
		.unwrap();

	fixture.await_holdings(vec![(holder(), HOLDER_ITEM, 6)]);
}

/// Keys allocated after the tracker started must be watched too, and only
/// they, since the keys before them are already watched.
#[test]
fn new_item_keys_are_watched_by_a_watch_of_their_own() {
	let mut fixture = tracker_after(|db| insert_item_key(db, holder(), 0));
	let first = fixture.next_watch();
	let count = std::num::NonZeroU32::new(1).unwrap();

	let allocated = block_on(fixture.ledger.allocate_items(PurseId(0), count)).expect("allocates");
	let second = fixture.next_watch();

	let [ItemKey(new_key)] = allocated[..] else { panic!("one key") };
	assert_eq!((first.keys.len(), second.keys.len()), (1, 1));
	assert!(second.keys[0].ends_with(&new_key), "{:?}", second.keys);
}

/// The cache survives a restart while the chain moves on, so the first
/// report must overwrite whatever an earlier run left behind.
#[test]
fn the_first_report_replaces_holdings_left_from_an_earlier_run() {
	let mut fixture = tracker_after(|db| {
		insert_item_key(db, holder(), 0);
		block_on(db.write(|tx| {
			tx.execute(
				"INSERT INTO nft_holding VALUES (?1, 1, 1, 1, 1, ?2)",
				rusqlite::params![holder(), [1u8; 32]],
			)?;
			Ok(())
		}))
		.expect("stale holding is written");
	});
	let watch = fixture.next_watch();

	watch.changes.unbounded_send(Ok(raw_changes_at(9, vec![(0, None)]))).unwrap();

	fixture.await_holdings(vec![]);
}
