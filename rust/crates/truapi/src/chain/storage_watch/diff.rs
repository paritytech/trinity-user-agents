//! Snapshots into changes: which raw values differ from the ones reported
//! last.

use core::future::ready;

use futures::stream::{BoxStream, StreamExt};
use subxt::ArcMetadata;

use super::StorageSnapshot;
use crate::{chain::HashAndNumber, chain_runtime::RuntimeFailure};

/// Raw values that changed since the snapshot reported last.
#[derive(Debug)]
pub struct RawChanges {
	/// Block of the newer snapshot.
	pub at: HashAndNumber,
	/// Metadata to decode the changed values with.
	pub metadata: ArcMetadata,
	/// Position of each changed key among the watched keys, and its value.
	pub changed: Vec<(usize, Option<Vec<u8>>)>,
}

/// The changes between consecutive `snapshots`. The first snapshot reports
/// every key, and a snapshot equal to the one before reports nothing. An
/// error passes through and leaves the earlier snapshot as the baseline.
pub fn snapshot_changes(
	snapshots: BoxStream<'static, Result<StorageSnapshot, RuntimeFailure>>,
) -> BoxStream<'static, Result<RawChanges, RuntimeFailure>> {
	snapshots
		.scan(None, |baseline, snapshot| ready(Some(next_changes(baseline, snapshot))))
		.filter_map(|changes| ready(changes.transpose()))
		.boxed()
}

/// The changes `snapshot` makes to `baseline`, the values reported last, which
/// it then replaces. `None` when nothing changed.
fn next_changes(
	baseline: &mut Option<Vec<Option<Vec<u8>>>>,
	snapshot: Result<StorageSnapshot, RuntimeFailure>,
) -> Result<Option<RawChanges>, RuntimeFailure> {
	let snapshot = snapshot?;
	let first = baseline.is_none();
	let changed = changed_values(baseline.as_deref(), &snapshot.values);
	*baseline = Some(snapshot.values);
	if changed.is_empty() && !first {
		return Ok(None);
	}
	Ok(Some(RawChanges { at: snapshot.at, metadata: snapshot.metadata, changed }))
}

/// Each value of `current` that differs from the same position in
/// `previous`, or every value when there is no `previous`.
fn changed_values(
	previous: Option<&[Option<Vec<u8>>]>,
	current: &[Option<Vec<u8>>],
) -> Vec<(usize, Option<Vec<u8>>)> {
	current
		.iter()
		.enumerate()
		.filter(|(index, value)| is_changed(previous, *index, value))
		.map(|(index, value)| (index, value.clone()))
		.collect()
}

fn is_changed(previous: Option<&[Option<Vec<u8>>]>, index: usize, value: &Option<Vec<u8>>) -> bool {
	let Some(previous) = previous else { return true };
	previous.get(index) != Some(value)
}
