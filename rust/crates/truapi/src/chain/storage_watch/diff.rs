//! Full snapshots into changes, for a source that can only read every key:
//! which values differ from the ones reported last. Only a hash of each
//! reported value is kept, so a watch of large values holds little memory.

use core::future::ready;

use futures::stream::{BoxStream, StreamExt};
use sp_crypto_hashing::blake2_256;

use super::{DecodableAtBlock, RawStorageChanges};
use crate::chain_runtime::RuntimeFailure;

/// The value of every watched key at one block, in request order and `None`
/// when absent.
pub type StorageSnapshot = DecodableAtBlock<Vec<Option<Vec<u8>>>>;

/// The Blake2-256 hash of a storage value, `None` for an absent entry, which
/// stands in for the value when comparing it with a later one.
pub fn hash_of_storage_value(value: &Option<Vec<u8>>) -> Option<[u8; 32]> {
	value.as_deref().map(blake2_256)
}

/// The changes between consecutive `snapshots`. The first snapshot reports
/// every key, and a snapshot equal to the one before reports nothing. An
/// error passes through and leaves the earlier snapshot as the baseline.
pub fn changes_between_consecutive_snapshots(
	snapshots: BoxStream<'static, Result<StorageSnapshot, RuntimeFailure>>,
) -> BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>> {
	snapshots
		.scan(None, |reported_hashes, snapshot| {
			let changes = snapshot.map(|snapshot| {
				changes_since_reported_hashes_updating_them(reported_hashes, snapshot)
			});
			ready(Some(changes))
		})
		.filter_map(|changes| ready(changes.transpose()))
		.boxed()
}

/// The values of `snapshot` whose hash differs from `reported_hashes`, which
/// then become the snapshot's hashes. Every value when nothing was reported
/// yet, `None` when nothing changed.
fn changes_since_reported_hashes_updating_them(
	reported_hashes: &mut Option<Vec<Option<[u8; 32]>>>,
	snapshot: StorageSnapshot,
) -> Option<RawStorageChanges> {
	let snapshot_hashes = snapshot.value.iter().map(hash_of_storage_value).collect();
	let Some(previous_hashes) = reported_hashes.replace(snapshot_hashes) else {
		return Some(snapshot.map_value(|values| values.into_iter().enumerate().collect()));
	};
	let changes =
		snapshot.map_value(|current| values_whose_hash_differs(&previous_hashes, current));
	(!changes.value.is_empty()).then_some(changes)
}

/// Each value of `current`, with its position, whose hash differs from the
/// hash at the same position in `previous_hashes`.
fn values_whose_hash_differs(
	previous_hashes: &[Option<[u8; 32]>],
	current: Vec<Option<Vec<u8>>>,
) -> Vec<(usize, Option<Vec<u8>>)> {
	current
		.into_iter()
		.enumerate()
		.filter(|(index, value)| {
			let current_hash = hash_of_storage_value(value);
			previous_hashes.get(*index) != Some(&current_hash)
		})
		.collect()
}
