//! Where the core keeps the last descriptor it verified for a stack, so the
//! next start does not go back to the bundled copy.

use trinity_stack::{Pointer, Refresh, StackReader};
use truapi::latest::GenericError;

/// The store of each stack's descriptor record. A record is opaque to the
/// store.
#[async_trait::async_trait]
pub trait StackRecords: Send + Sync {
	/// The record stored for `stack_id`, or `None` when there is none.
	/// Read once, while the runtime is built.
	async fn read_stack_record(&self, stack_id: &str)
	-> Result<Option<Vec<u8>>, GenericError>;

	/// Stores `record` for `stack_id`, unless `check` says the record already
	/// there holds a newer copy. Two runtimes can refresh at once while a host
	/// rebuilds its runtime, and the slower one must not put an older copy
	/// back. A store checks and writes in one transaction where it can.
	async fn write_stack_record(
		&self,
		stack_id: &str,
		record: Vec<u8>,
		check: ReplaceCheck,
	) -> Result<(), GenericError>;
}

/// Whether a record may replace the one already stored for its stack: not
/// when the stored one holds a higher `sequence`. A stored record that no
/// longer reads is replaced.
pub struct ReplaceCheck {
	pointer: Pointer,
	sequence: Option<u32>,
}

impl ReplaceCheck {
	/// The check for storing `record`, a record of the stack `pointer` names.
	pub fn new(pointer: Pointer, record: &[u8]) -> Self {
		let sequence = record_sequence(&pointer, record);
		Self { pointer, sequence }
	}

	/// Whether the record may replace `existing`.
	#[must_use]
	pub fn replaces(&self, existing: &[u8]) -> bool {
		record_sequence(&self.pointer, existing) <= self.sequence
	}
}

/// The `sequence` of the copy in `record`, or `None` when it no longer reads.
fn record_sequence(pointer: &Pointer, record: &[u8]) -> Option<u32> {
	let mut reader = StackReader::new(Refresh::Never);
	reader.load(pointer, None, Some(record)).ok()?;
	Some(reader.get(pointer, 0).current?.stack.descriptor.sequence)
}
