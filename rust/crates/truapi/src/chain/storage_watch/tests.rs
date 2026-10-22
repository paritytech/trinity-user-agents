use std::sync::{Arc, Mutex};

use futures::{
	StreamExt,
	channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
	executor::block_on,
	stream::{self, BoxStream},
};
use parity_scale_codec::Encode;
use sp_crypto_hashing::{blake2_128, twox_64, twox_128};
use subxt::{
	ArcMetadata,
	client::{OfflineClientAtBlock, OnlineClientAtBlock},
	config::SubstrateConfig,
	storage::dynamic,
	utils::H256,
};

use super::{
	DecodableAtBlock, DecodedEntryChanges, RETRY_DELAY, RawStorageChanges, StorageSnapshot,
	StorageWatchError, WatchedHead, WatchedStorage,
	codec::{decode_value_or_default, encode_storage_keys},
	diffed,
};
use crate::{
	chain::{
		HashAndNumber,
		tests::{Node, serve},
	},
	chain_runtime::{ChainRuntime, RuntimeFailure},
	host_internal::extrinsic::tests::{bulletin_chain_state, bulletin_metadata},
	time::{ManualTimer, Timer},
};

const GENESIS: H256 = H256([0xab; 32]);

fn bulletin_fixture_metadata() -> ArcMetadata {
	ArcMetadata::from(bulletin_metadata())
}

/// An offline client over the bulletin fixture, for encoding keys.
fn bulletin_fixture_client() -> OfflineClientAtBlock<SubstrateConfig> {
	bulletin_chain_state().client_at(0).unwrap()
}

fn test_block(number: u64) -> HashAndNumber {
	HashAndNumber { hash: H256::repeat_byte(u8::try_from(number).unwrap()), number }
}

fn snapshot_at_block(
	number: u64,
	values: &[Option<&[u8]>],
) -> Result<StorageSnapshot, RuntimeFailure> {
	let values = values.iter().map(|value| value.map(<[u8]>::to_vec)).collect();
	Ok(DecodableAtBlock {
		at: test_block(number),
		metadata: bulletin_fixture_metadata(),
		value: values,
	})
}

fn scripted_read_failure() -> Result<StorageSnapshot, RuntimeFailure> {
	Err(RuntimeFailure::host_failure("storage_snapshot", "scripted failure"))
}

/// What one raw change reported: its block number and its changed
/// positions.
#[derive(Debug, PartialEq)]
struct ReportedRawChanges {
	number: u64,
	changed: Vec<(usize, Option<Vec<u8>>)>,
}

fn reported_raw_changes_at(
	number: u64,
	changed: Vec<(usize, Option<Vec<u8>>)>,
) -> ReportedRawChanges {
	ReportedRawChanges { number, changed }
}

fn raw_changes_reported_for(
	snapshots: Vec<Result<StorageSnapshot, RuntimeFailure>>,
) -> Vec<Result<ReportedRawChanges, RuntimeFailure>> {
	let changes = diffed(stream::iter(snapshots).boxed());
	let collected: Vec<Result<RawStorageChanges, RuntimeFailure>> = block_on(changes.collect());
	collected
		.into_iter()
		.map(|raw| raw.map(|raw| reported_raw_changes_at(raw.at.number, raw.value)))
		.collect()
}

#[test]
fn the_first_snapshot_reports_every_key_including_absent_ones() {
	let reported = raw_changes_reported_for(vec![snapshot_at_block(1, &[Some(b"a"), None])]);

	assert_eq!(
		reported,
		vec![Ok(reported_raw_changes_at(1, vec![(0, Some(b"a".to_vec())), (1, None)]))]
	);
}

#[test]
fn a_snapshot_equal_to_the_one_before_reports_nothing() {
	let reported = raw_changes_reported_for(vec![
		snapshot_at_block(1, &[Some(b"a")]),
		snapshot_at_block(2, &[Some(b"a")]),
	]);

	assert_eq!(reported, vec![Ok(reported_raw_changes_at(1, vec![(0, Some(b"a".to_vec()))]))]);
}

#[test]
fn only_the_keys_that_changed_are_reported() {
	let reported = raw_changes_reported_for(vec![
		snapshot_at_block(1, &[Some(b"a"), None, Some(b"c")]),
		snapshot_at_block(2, &[None, Some(b"b"), Some(b"c")]),
	]);

	assert_eq!(
		reported[1],
		Ok(reported_raw_changes_at(2, vec![(0, None), (1, Some(b"b".to_vec()))]))
	);
}

#[test]
fn a_value_going_back_to_an_earlier_one_is_a_change() {
	// After a reorg of the best chain the new fork can hold a value reported
	// two reads ago; the consumer must hear about it.
	let reported = raw_changes_reported_for(vec![
		snapshot_at_block(1, &[Some(b"a")]),
		snapshot_at_block(2, &[Some(b"b")]),
		snapshot_at_block(3, &[Some(b"a")]),
	]);

	assert_eq!(reported[2], Ok(reported_raw_changes_at(3, vec![(0, Some(b"a".to_vec()))])));
}

#[test]
fn a_failed_read_passes_through_and_keeps_the_values_reported_before_it() {
	// A failure is never an absent value, and the read after it is compared
	// with what the consumer last heard, so nothing is reported twice.
	let reported = raw_changes_reported_for(vec![
		snapshot_at_block(1, &[Some(b"a"), Some(b"b")]),
		scripted_read_failure(),
		snapshot_at_block(3, &[Some(b"a"), None]),
	]);

	assert_eq!(reported.len(), 3);
	assert!(reported[1].is_err());
	assert_eq!(reported[2], Ok(reported_raw_changes_at(3, vec![(1, None)])));
}

#[test]
fn keys_encode_as_the_chain_stores_them() {
	let account = [7u8; 32];
	let address = dynamic::<([u8; 32],), ()>("System", "Account");

	let encoded = encode_storage_keys(&address, &[(account,)], &bulletin_fixture_client()).unwrap();

	let expected =
		[&twox_128(b"System")[..], &twox_128(b"Account"), &blake2_128(&account), &account].concat();
	assert_eq!(encoded, vec![expected]);
}

#[test]
fn a_key_with_the_wrong_number_of_parts_names_its_position() {
	let address = dynamic::<(u32, u32), ()>("System", "BlockHash");

	let error = encode_storage_keys(&address, &[(1, 2)], &bulletin_fixture_client()).unwrap_err();

	let message = error.to_string();
	assert!(message.starts_with("key 0 of System.BlockHash did not encode: "), "{message}");
	assert!(message.contains("expected 1 key parts but got 2"), "{message}");
}

#[test]
fn an_entry_missing_from_the_metadata_is_incompatible() {
	let address = dynamic::<(u32,), ()>("System", "NoSuchEntry");

	let error = encode_storage_keys(&address, &[(1,)], &bulletin_fixture_client()).unwrap_err();

	assert!(matches!(error, StorageWatchError::Incompatible { .. }), "{error}");
}

#[test]
fn an_absent_entry_decodes_to_its_default_or_to_none_without_one() {
	// `System.Number` is a `ValueQuery` with default 0, and
	// `System.LastRuntimeUpgrade` an `OptionQuery`: subxt's own `try_fetch`
	// answers the same.
	let number = dynamic::<(), u32>("System", "Number");
	let upgrade = dynamic::<(), subxt::ext::scale_value::Value>("System", "LastRuntimeUpgrade");

	let number =
		decode_value_or_default(&number, None, &bulletin_fixture_metadata(), test_block(1))
			.unwrap();
	let upgrade =
		decode_value_or_default(&upgrade, None, &bulletin_fixture_metadata(), test_block(1))
			.unwrap();

	assert_eq!(number, Some(0));
	assert!(upgrade.is_none());
}

#[test]
fn bytes_that_do_not_decode_name_the_entry_and_block() {
	let address = dynamic::<(), u32>("System", "Number");

	let error = decode_value_or_default(
		&address,
		Some(&[1, 2]),
		&bulletin_fixture_metadata(),
		test_block(4),
	)
	.unwrap_err();

	let message = error.to_string();
	assert!(message.starts_with("System.Number at block 4 ("), "{message}");
}

#[test]
fn bytes_left_over_after_the_value_are_an_error() {
	let address = dynamic::<(), u32>("System", "Number");

	let error = decode_value_or_default(
		&address,
		Some(&[1, 0, 0, 0, 9]),
		&bulletin_fixture_metadata(),
		test_block(4),
	)
	.unwrap_err();

	assert!(error.to_string().ends_with("did not decode: 1 of 5 bytes left over"), "{error}");
}

/// Changes a test scripts by hand, and the keys the watch asked for. The
/// client for encoding keys comes from a scripted node serving the bulletin
/// runtime.
struct ScriptedWatchedStorage {
	chain: ChainRuntime,
	changes: Mutex<Option<UnboundedReceiver<Result<RawStorageChanges, RuntimeFailure>>>>,
	requested: Mutex<Option<(WatchedHead, Vec<Vec<u8>>)>>,
	/// How many more watches fail to open, as if the chain were unreachable.
	unreachable_opens: Mutex<usize>,
	/// The clock a watch waits on between attempts, which the test moves.
	timer: Arc<ManualTimer>,
}

impl ScriptedWatchedStorage {
	fn new() -> (Arc<Self>, UnboundedSender<Result<RawStorageChanges, RuntimeFailure>>) {
		Self::unreachable_for(0)
	}

	/// Whether this watch still fails to open, counting it as one of those.
	fn take_unreachable_open(&self) -> bool {
		let mut left = self.unreachable_opens.lock().unwrap();
		let unreachable = *left > 0;
		*left = left.saturating_sub(1);
		unreachable
	}

	/// A fake whose first `opens` watches fail to open.
	fn unreachable_for(
		opens: usize,
	) -> (Arc<Self>, UnboundedSender<Result<RawStorageChanges, RuntimeFailure>>) {
		let (sender, receiver) = unbounded();
		let (_node, _provider, chain) = serve(Node::new(3, 5));
		let changes = Mutex::new(Some(receiver));
		let unreachable_opens = Mutex::new(opens);
		let timer = Arc::new(ManualTimer::default());
		let requested = Mutex::new(None);
		let fake = Arc::new(Self { chain, changes, requested, unreachable_opens, timer });
		(fake, sender)
	}
}

#[async_trait::async_trait]
impl WatchedStorage for ScriptedWatchedStorage {
	async fn client_at_finalized_head(
		&self,
		genesis: H256,
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, RuntimeFailure> {
		if self.take_unreachable_open() {
			return Err(RuntimeFailure::host_failure(
				"test",
				"the chain is unreachable".to_owned(),
			));
		}
		self.chain.client_at_finalized_head(genesis).await
	}

	async fn watch_raw_key_changes(
		&self,
		_genesis: H256,
		head: WatchedHead,
		keys: Vec<Vec<u8>>,
	) -> Result<BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>, RuntimeFailure> {
		*self.requested.lock().unwrap() = Some((head, keys));
		let receiver = self.changes.lock().unwrap().take().expect("one watch per fake");
		Ok(receiver.boxed())
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
		self.timer.clone()
	}
}

fn scripted_raw_changes_at(
	number: u64,
	changed: &[(usize, Option<&[u8]>)],
) -> Result<RawStorageChanges, RuntimeFailure> {
	let changed = changed
		.iter()
		.map(|(index, value)| (*index, value.map(<[u8]>::to_vec)))
		.collect();
	Ok(DecodableAtBlock {
		at: test_block(number),
		metadata: bulletin_fixture_metadata(),
		value: changed,
	})
}

/// `System.BlockHash` of `number`: a `Twox64Concat` map over `u32`.
fn system_block_hash_storage_key(number: u32) -> Vec<u8> {
	let encoded = number.encode();
	[&twox_128(b"System")[..], &twox_128(b"BlockHash"), &twox_64(&encoded), &encoded].concat()
}

#[test]
fn a_watch_reads_the_encoded_keys_and_reports_decoded_values_under_the_callers_keys() {
	let (fake, sender) = ScriptedWatchedStorage::new();
	let storage: Arc<dyn WatchedStorage> = fake.clone();
	let address = dynamic::<(u32,), [u8; 32]>("System", "BlockHash");
	let watch = storage.start_entry_watch(GENESIS, WatchedHead::Best, address, vec![(1,), (2,)]);
	let mut changes = block_on(watch).unwrap();
	sender
		.unbounded_send(scripted_raw_changes_at(5, &[(0, Some(&[1; 32])), (1, None)]))
		.unwrap();
	sender
		.unbounded_send(scripted_raw_changes_at(6, &[(1, Some(&[2; 32]))]))
		.unwrap();

	let first = block_on(changes.next()).unwrap().unwrap();
	let second = block_on(changes.next()).unwrap().unwrap();

	let requested = fake.requested.lock().unwrap().clone();
	assert_eq!(
		requested,
		Some((
			WatchedHead::Best,
			vec![system_block_hash_storage_key(1), system_block_hash_storage_key(2)]
		))
	);
	assert_eq!(
		first,
		DecodedEntryChanges {
			at: test_block(5),
			changed: vec![((1,), Some([1; 32])), ((2,), Some([0; 32]))]
		}
	);
	assert_eq!(
		second,
		DecodedEntryChanges { at: test_block(6), changed: vec![((2,), Some([2; 32]))] }
	);
}

#[test]
fn a_failed_read_is_an_error_item_and_the_watch_goes_on() {
	let (fake, sender) = ScriptedWatchedStorage::new();
	let storage: Arc<dyn WatchedStorage> = fake;
	let address = dynamic::<(), u32>("System", "Number");
	let mut changes =
		block_on(storage.start_entry_watch(GENESIS, WatchedHead::Finalized, address, vec![()]))
			.unwrap();
	sender
		.unbounded_send(Err(RuntimeFailure::host_failure("storage_watch", "scripted failure")))
		.unwrap();
	sender
		.unbounded_send(scripted_raw_changes_at(2, &[(0, Some(&7u32.encode()))]))
		.unwrap();

	let failed = block_on(changes.next()).unwrap();
	let read = block_on(changes.next()).unwrap().unwrap();

	assert!(matches!(failed, Err(StorageWatchError::Chain(_))));
	assert_eq!(read, DecodedEntryChanges { at: test_block(2), changed: vec![((), Some(7))] });
}

#[test]
fn metadata_without_the_watched_entry_fails_the_watch_before_it_starts() {
	let (fake, _sender) = ScriptedWatchedStorage::new();
	let storage: Arc<dyn WatchedStorage> = fake.clone();
	let address = dynamic::<(), u32>("NoSuchPallet", "Number");

	let watch =
		block_on(storage.start_entry_watch(GENESIS, WatchedHead::Finalized, address, vec![()]));

	assert!(matches!(watch, Err(StorageWatchError::Incompatible { .. })));
	assert_eq!(*fake.requested.lock().unwrap(), None);
}

/// A watch must not lose its keys to an unreachable chain, nor retry it
/// without pause: it reports why it could not start, and starts again only
/// once the delay has passed.
#[test]
fn a_watch_that_cannot_start_reports_why_and_retries_after_the_delay() {
	let (fake, sender) = ScriptedWatchedStorage::unreachable_for(1);
	let timer = fake.timer.clone();
	let storage: Arc<dyn WatchedStorage> = fake;
	let address = dynamic::<(), u32>("System", "Number");
	let mut changes = storage.watch_entry_changes(GENESIS, WatchedHead::Best, address, vec![()]);
	let failed = block_on(changes.next()).expect("an item");
	let mut restarted = changes.next();
	let pending = block_on(async { futures::poll!(&mut restarted) }).is_pending();
	let sleeps_before_restarting = timer.sleeps();

	timer.advance(RETRY_DELAY);
	sender
		.unbounded_send(scripted_raw_changes_at(2, &[(0, Some(&7u32.encode()))]))
		.unwrap();
	let read = block_on(restarted).expect("an item").expect("a read");

	assert!(matches!(failed, Err(StorageWatchError::Chain(_))), "{failed:?}");
	assert!(pending);
	assert_eq!(sleeps_before_restarting, 1);
	assert_eq!(read, DecodedEntryChanges { at: test_block(2), changed: vec![((), Some(7))] });
}

/// A runtime that does not declare the entry fails every attempt the same
/// way, so the watch says why once and ends instead of retrying forever.
#[test]
fn a_watch_of_an_entry_the_runtime_lacks_reports_why_and_ends() {
	let (fake, _sender) = ScriptedWatchedStorage::new();
	let timer = fake.timer.clone();
	let storage: Arc<dyn WatchedStorage> = fake;
	let address = dynamic::<(), u32>("NoSuchPallet", "Number");

	let mut changes = storage.watch_entry_changes(GENESIS, WatchedHead::Best, address, vec![()]);
	let first = block_on(changes.next());
	let ended = block_on(async { futures::poll!(changes.next()) }).is_ready();

	assert!(matches!(first, Some(Err(StorageWatchError::Incompatible { .. }))), "{first:?}");
	assert!(ended);
	assert_eq!(timer.sleeps(), 0);
}
