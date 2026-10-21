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

/// The Blake2-256 hash of a storage value, `None` for an absent entry. It
/// stands in for the value when comparing it with a later one.
pub fn value_hash(value: &Option<Vec<u8>>) -> Option<[u8; 32]> {
	value.as_deref().map(blake2_256)
}

/// The changes between consecutive `snapshots`. The first snapshot reports
/// every key, and a snapshot equal to the one before reports nothing. An
/// error passes through and leaves the earlier snapshot as the baseline.
pub fn diffed(
	snapshots: BoxStream<'static, Result<StorageSnapshot, RuntimeFailure>>,
) -> BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>> {
	snapshots
		.scan(None, |previous_hashes: &mut Option<Vec<Option<[u8; 32]>>>, snapshot| {
			let changes = snapshot.map(|snapshot| {
				let current_hashes = snapshot.value.iter().map(value_hash).collect();
				let changes = find_changes(previous_hashes.as_deref(), snapshot);
				*previous_hashes = Some(current_hashes);
				changes
			});
			ready(Some(changes))
		})
		.filter_map(|changes| ready(changes.transpose()))
		.boxed()
}

/// The values of `snapshot` that changed since the snapshot with
/// `previous_hashes`: every value when there is none, `None` when nothing
/// changed.
fn find_changes(
	previous_hashes: Option<&[Option<[u8; 32]>]>,
	snapshot: StorageSnapshot,
) -> Option<RawStorageChanges> {
	let Some(previous_hashes) = previous_hashes else {
		return Some(snapshot.map_value(|values| values.into_iter().enumerate().collect()));
	};
	let changes = snapshot.map_value(|current| find_changed_values(previous_hashes, current));
	(!changes.value.is_empty()).then_some(changes)
}

/// Each value of `current`, with its position, whose hash differs from the
/// one at the same position in `previous_hashes`.
fn find_changed_values(
	previous_hashes: &[Option<[u8; 32]>],
	current: Vec<Option<Vec<u8>>>,
) -> Vec<(usize, Option<Vec<u8>>)> {
	current
		.into_iter()
		.enumerate()
		.filter(|(index, value)| {
			let current_hash = value_hash(value);
			previous_hashes.get(*index) != Some(&current_hash)
		})
		.collect()
}
