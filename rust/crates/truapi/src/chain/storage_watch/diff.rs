//! Snapshots into changes: which raw values differ from the ones reported
//! last.

use core::future::ready;

use futures::stream::{BoxStream, StreamExt};

use super::{DecodableAtBlock, StorageSnapshot};
use crate::chain_runtime::RuntimeFailure;

/// The position of each changed key among the watched keys, and its new
/// value, at one block.
pub type RawChanges = DecodableAtBlock<Vec<(usize, Option<Vec<u8>>)>>;

/// The changes between consecutive `snapshots`. The first snapshot reports
/// every key, and a snapshot equal to the one before reports nothing. An
/// error passes through and leaves the earlier snapshot as the baseline.
pub fn snapshot_changes(
	snapshots: BoxStream<'static, Result<StorageSnapshot, RuntimeFailure>>,
) -> BoxStream<'static, Result<RawChanges, RuntimeFailure>> {
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
) -> Option<RawChanges> {
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
