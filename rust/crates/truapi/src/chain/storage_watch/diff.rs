//! Full snapshots into changes, for a source that can only read every key:
//! which values differ from the ones reported last.

use core::future::ready;

use futures::stream::{BoxStream, StreamExt};

use super::{DecodableAtBlock, RawStorageChanges};
use crate::chain_runtime::RuntimeFailure;

/// The value of every watched key at one block, in request order and `None`
/// when absent.
pub type StorageSnapshot = DecodableAtBlock<Vec<Option<Vec<u8>>>>;

/// The changes between consecutive `snapshots`. The first snapshot reports
/// every key, and a snapshot equal to the one before reports nothing. An
/// error passes through and leaves the earlier snapshot as the baseline.
pub fn snapshot_changes(
	snapshots: BoxStream<'static, Result<StorageSnapshot, RuntimeFailure>>,
) -> BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>> {
	snapshots
		.scan(None, |reported, snapshot| {
			let changes = snapshot.map(|snapshot| next_changes(reported, snapshot));
			ready(Some(changes))
		})
		.filter_map(|changes| ready(changes.transpose()))
		.boxed()
}

/// The changes `snapshot` makes to the values `reported` last, which it then
/// replaces. `None` when nothing changed.
fn next_changes(
	reported: &mut Option<Vec<Option<Vec<u8>>>>,
	snapshot: StorageSnapshot,
) -> Option<RawStorageChanges> {
	let Some(previous) = reported.replace(snapshot.value.clone()) else {
		return Some(snapshot.map(|values| values.into_iter().enumerate().collect()));
	};
	let changes = snapshot.map(|current| changed_values(&previous, current));
	(!changes.value.is_empty()).then_some(changes)
}

/// Each value of `current` that differs from the same position in `previous`.
fn changed_values(
	previous: &[Option<Vec<u8>>],
	current: Vec<Option<Vec<u8>>>,
) -> Vec<(usize, Option<Vec<u8>>)> {
	current
		.into_iter()
		.enumerate()
		.filter(|(index, value)| previous.get(*index) != Some(value))
		.collect()
}
