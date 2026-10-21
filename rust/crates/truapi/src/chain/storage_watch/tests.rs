use std::sync::{Arc, Mutex};

use futures::{
	StreamExt,
	channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
	executor::block_on,
	stream::{self, BoxStream},
};
use parity_scale_codec::Encode;
use sp_crypto_hashing::{blake2_128, twox_64, twox_128};
use subxt::{ArcMetadata, storage::dynamic, utils::H256};

use super::{
	DecodableAtBlock, EntryChanges, RawStorageChanges, StorageSnapshot, StorageWatchError,
	WatchedHead, WatchedStorage,
	codec::{decode_value, encode_keys},
	snapshot_changes,
};
use crate::{
	chain::HashAndNumber, chain_runtime::RuntimeFailure,
	host_internal::extrinsic::tests::bulletin_metadata,
};

const GENESIS: H256 = H256([0xab; 32]);

fn metadata() -> ArcMetadata {
	ArcMetadata::from(bulletin_metadata())
}

fn block(number: u64) -> HashAndNumber {
	HashAndNumber { hash: H256::repeat_byte(u8::try_from(number).unwrap()), number }
}

fn snapshot(number: u64, values: &[Option<&[u8]>]) -> Result<StorageSnapshot, RuntimeFailure> {
	let values = values.iter().map(|value| value.map(<[u8]>::to_vec)).collect();
	Ok(DecodableAtBlock { at: block(number), metadata: metadata(), value: values })
}

fn read_failure() -> Result<StorageSnapshot, RuntimeFailure> {
	Err(RuntimeFailure::host_failure("storage_snapshot", "scripted failure"))
}

/// What one raw change reported: its block number and its changed
/// positions.
#[derive(Debug, PartialEq)]
struct Reported {
	number: u64,
	changed: Vec<(usize, Option<Vec<u8>>)>,
}

fn reported_at(number: u64, changed: Vec<(usize, Option<Vec<u8>>)>) -> Reported {
	Reported { number, changed }
}

fn reported(
	snapshots: Vec<Result<StorageSnapshot, RuntimeFailure>>,
) -> Vec<Result<Reported, RuntimeFailure>> {
	let changes = snapshot_changes(stream::iter(snapshots).boxed());
	let collected: Vec<Result<RawStorageChanges, RuntimeFailure>> = block_on(changes.collect());
	collected
		.into_iter()
		.map(|raw| raw.map(|raw| reported_at(raw.at.number, raw.value)))
		.collect()
}

#[test]
fn the_first_snapshot_reports_every_key_including_absent_ones() {
	let reported = reported(vec![snapshot(1, &[Some(b"a"), None])]);

	assert_eq!(reported, vec![Ok(reported_at(1, vec![(0, Some(b"a".to_vec())), (1, None)]))]);
}

#[test]
fn a_snapshot_equal_to_the_one_before_reports_nothing() {
	let reported = reported(vec![snapshot(1, &[Some(b"a")]), snapshot(2, &[Some(b"a")])]);

	assert_eq!(reported, vec![Ok(reported_at(1, vec![(0, Some(b"a".to_vec()))]))]);
}

#[test]
fn only_the_keys_that_changed_are_reported() {
	let reported = reported(vec![
		snapshot(1, &[Some(b"a"), None, Some(b"c")]),
		snapshot(2, &[None, Some(b"b"), Some(b"c")]),
	]);

	assert_eq!(reported[1], Ok(reported_at(2, vec![(0, None), (1, Some(b"b".to_vec()))])));
}

#[test]
fn a_value_going_back_to_an_earlier_one_is_a_change() {
	// After a reorg of the best chain the new fork can hold a value reported
	// two reads ago; the consumer must hear about it.
	let reported = reported(vec![
		snapshot(1, &[Some(b"a")]),
		snapshot(2, &[Some(b"b")]),
		snapshot(3, &[Some(b"a")]),
	]);

	assert_eq!(reported[2], Ok(reported_at(3, vec![(0, Some(b"a".to_vec()))])));
}

#[test]
fn a_failed_read_passes_through_and_keeps_the_values_reported_before_it() {
	// A failure is never an absent value, and the read after it is compared
	// with what the consumer last heard, so nothing is reported twice.
	let reported = reported(vec![
		snapshot(1, &[Some(b"a"), Some(b"b")]),
		read_failure(),
		snapshot(3, &[Some(b"a"), None]),
	]);

	assert_eq!(reported.len(), 3);
	assert!(reported[1].is_err());
	assert_eq!(reported[2], Ok(reported_at(3, vec![(1, None)])));
}

#[test]
fn keys_encode_as_the_chain_stores_them() {
	let account = [7u8; 32];
	let address = dynamic::<([u8; 32],), ()>("System", "Account");

	let encoded = encode_keys(&address, &[(account,)], &metadata()).unwrap();

	let expected =
		[&twox_128(b"System")[..], &twox_128(b"Account"), &blake2_128(&account), &account].concat();
	assert_eq!(encoded, vec![expected]);
}

#[test]
fn a_key_with_the_wrong_number_of_parts_names_its_position() {
	let address = dynamic::<(u32, u32), ()>("System", "BlockHash");

	let error = encode_keys(&address, &[(1, 2)], &metadata()).unwrap_err();

	assert_eq!(
		error.to_string(),
		"key 0 of System.BlockHash did not encode: expected 1 key parts, got 2"
	);
}

#[test]
fn an_entry_missing_from_the_metadata_is_incompatible() {
	let address = dynamic::<(u32,), ()>("System", "NoSuchEntry");

	let error = encode_keys(&address, &[(1,)], &metadata()).unwrap_err();

	assert!(matches!(error, StorageWatchError::Incompatible { .. }), "{error}");
}

#[test]
fn an_absent_entry_decodes_to_its_default_or_to_none_without_one() {
	// `System.Number` is a `ValueQuery` with default 0, and
	// `System.LastRuntimeUpgrade` an `OptionQuery`: subxt's own `try_fetch`
	// answers the same.
	let number = dynamic::<(), u32>("System", "Number");
	let upgrade = dynamic::<(), subxt::ext::scale_value::Value>("System", "LastRuntimeUpgrade");

	let number = decode_value(&number, None, &metadata(), block(1)).unwrap();
	let upgrade = decode_value(&upgrade, None, &metadata(), block(1)).unwrap();

	assert_eq!((number, upgrade.is_none()), (Some(0), true));
}

#[test]
fn bytes_that_do_not_decode_name_the_entry_and_block() {
	let address = dynamic::<(), u32>("System", "Number");

	let error = decode_value(&address, Some(&[1, 2]), &metadata(), block(4)).unwrap_err();

	let message = error.to_string();
	assert!(message.starts_with("System.Number at block 4 ("), "{message}");
}

#[test]
fn bytes_left_over_after_the_value_are_an_error() {
	let address = dynamic::<(), u32>("System", "Number");

	let error = decode_value(&address, Some(&[1, 0, 0, 0, 9]), &metadata(), block(4)).unwrap_err();

	assert!(error.to_string().ends_with("did not decode: 1 of 5 bytes left over"), "{error}");
}

/// Changes a test scripts by hand, and the keys the watch asked for.
struct FakeStorage {
	changes: Mutex<Option<UnboundedReceiver<Result<RawStorageChanges, RuntimeFailure>>>>,
	requested: Mutex<Option<(WatchedHead, Vec<Vec<u8>>)>>,
}

impl FakeStorage {
	fn new() -> (Arc<Self>, UnboundedSender<Result<RawStorageChanges, RuntimeFailure>>) {
		let (sender, receiver) = unbounded();
		let fake =
			Arc::new(Self { changes: Mutex::new(Some(receiver)), requested: Mutex::new(None) });
		(fake, sender)
	}
}

#[async_trait::async_trait]
impl WatchedStorage for FakeStorage {
	async fn current_metadata(&self, _genesis: H256) -> Result<ArcMetadata, RuntimeFailure> {
		Ok(metadata())
	}

	async fn changes(
		&self,
		_genesis: H256,
		head: WatchedHead,
		keys: Vec<Vec<u8>>,
	) -> Result<BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>, RuntimeFailure> {
		*self.requested.lock().unwrap() = Some((head, keys));
		let receiver = self.changes.lock().unwrap().take().expect("one watch per fake");
		Ok(receiver.boxed())
	}
}

fn changes_at(
	number: u64,
	changed: &[(usize, Option<&[u8]>)],
) -> Result<RawStorageChanges, RuntimeFailure> {
	let changed = changed
		.iter()
		.map(|(index, value)| (*index, value.map(<[u8]>::to_vec)))
		.collect();
	Ok(DecodableAtBlock { at: block(number), metadata: metadata(), value: changed })
}

/// `System.BlockHash` of `number`: a `Twox64Concat` map over `u32`.
fn block_hash_key(number: u32) -> Vec<u8> {
	let encoded = number.encode();
	[&twox_128(b"System")[..], &twox_128(b"BlockHash"), &twox_64(&encoded), &encoded].concat()
}

#[test]
fn a_watch_reads_the_encoded_keys_and_reports_decoded_values_under_the_callers_keys() {
	let (fake, sender) = FakeStorage::new();
	let storage: Arc<dyn WatchedStorage> = fake.clone();
	let address = dynamic::<(u32,), [u8; 32]>("System", "BlockHash");
	let watch = storage.watch_entry(GENESIS, WatchedHead::Best, address, vec![(1,), (2,)]);
	let mut changes = block_on(watch).unwrap();
	sender.unbounded_send(changes_at(5, &[(0, Some(&[1; 32])), (1, None)])).unwrap();
	sender.unbounded_send(changes_at(6, &[(1, Some(&[2; 32]))])).unwrap();

	let first = block_on(changes.next()).unwrap().unwrap();
	let second = block_on(changes.next()).unwrap().unwrap();

	let requested = fake.requested.lock().unwrap().clone();
	assert_eq!(requested, Some((WatchedHead::Best, vec![block_hash_key(1), block_hash_key(2)])));
	assert_eq!(
		first,
		EntryChanges { at: block(5), changed: vec![((1,), Some([1; 32])), ((2,), Some([0; 32]))] }
	);
	assert_eq!(second, EntryChanges { at: block(6), changed: vec![((2,), Some([2; 32]))] });
}

#[test]
fn a_failed_read_is_an_error_item_and_the_watch_goes_on() {
	let (fake, sender) = FakeStorage::new();
	let storage: Arc<dyn WatchedStorage> = fake;
	let address = dynamic::<(), u32>("System", "Number");
	let mut changes =
		block_on(storage.watch_entry(GENESIS, WatchedHead::Finalized, address, vec![()])).unwrap();
	sender
		.unbounded_send(Err(RuntimeFailure::host_failure("storage_watch", "scripted failure")))
		.unwrap();
	sender.unbounded_send(changes_at(2, &[(0, Some(&7u32.encode()))])).unwrap();

	let failed = block_on(changes.next()).unwrap();
	let read = block_on(changes.next()).unwrap().unwrap();

	assert!(matches!(failed, Err(StorageWatchError::Chain(_))));
	assert_eq!(read, EntryChanges { at: block(2), changed: vec![((), Some(7))] });
}

#[test]
fn metadata_without_the_watched_entry_fails_the_watch_before_it_starts() {
	let (fake, _sender) = FakeStorage::new();
	let storage: Arc<dyn WatchedStorage> = fake.clone();
	let address = dynamic::<(), u32>("NoSuchPallet", "Number");

	let watch = block_on(storage.watch_entry(GENESIS, WatchedHead::Finalized, address, vec![()]));

	assert!(matches!(watch, Err(StorageWatchError::Incompatible { .. })));
	assert_eq!(*fake.requested.lock().unwrap(), None);
}
