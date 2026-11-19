//! Where the core keeps the last descriptor it verified for a stack, so the
//! next start does not go back to the bundled copy.

use std::sync::Arc;

use trinity_stack::{Pointer, Refresh, StackReader};
use truapi::latest::GenericError;

use crate::platform::{CoreStorage, CoreStorageKey};

/// The store of each stack's descriptor record. A record is opaque to the
/// store.
#[async_trait::async_trait]
pub trait StackRecords: Send + Sync {
	/// The record stored for `stack_id`, or `None` when there is none.
	/// Read once, while the runtime is built.
	async fn read_stack_record(&self, stack_id: &str) -> Result<Option<Vec<u8>>, GenericError>;

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

/// The stack's record kept in a host's [`CoreStorage`], under
/// [`CoreStorageKey::StackRecord`]. A browser core keeps it there with the
/// rest of its data; a native core uses its own database instead.
pub struct CoreStorageRecords(Arc<dyn CoreStorage>);

impl CoreStorageRecords {
	/// The record store in `storage`.
	pub fn new(storage: Arc<dyn CoreStorage>) -> Self {
		Self(storage)
	}
}

#[async_trait::async_trait]
impl StackRecords for CoreStorageRecords {
	async fn read_stack_record(&self, stack_id: &str) -> Result<Option<Vec<u8>>, GenericError> {
		self.0
			.read_core_storage(CoreStorageKey::StackRecord { stack_id: stack_id.to_string() })
			.await
	}

	/// Reads the stored record before it writes. The two steps are not one
	/// transaction, since core storage has none, so a second runtime writing
	/// in between can still be overwritten.
	async fn write_stack_record(
		&self,
		stack_id: &str,
		record: Vec<u8>,
		check: ReplaceCheck,
	) -> Result<(), GenericError> {
		let key = CoreStorageKey::StackRecord { stack_id: stack_id.to_string() };
		let stored = self.0.read_core_storage(key.clone()).await?;
		if stored.is_some_and(|stored| !check.replaces(&stored)) {
			return Ok(());
		}
		self.0.write_core_storage(key, record).await
	}
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl StackRecords for crate::store::Db {
	async fn read_stack_record(&self, stack_id: &str) -> Result<Option<Vec<u8>>, GenericError> {
		let stack_id = stack_id.to_string();
		self.read(move |conn| Ok(stored_record(conn, &stack_id)?))
			.await
			.map_err(|error| error.to_string().into())
	}

	async fn write_stack_record(
		&self,
		stack_id: &str,
		record: Vec<u8>,
		check: ReplaceCheck,
	) -> Result<(), GenericError> {
		let stack_id = stack_id.to_string();
		self.write(move |tx| {
			let stored = stored_record(tx, &stack_id)?;
			if stored.is_some_and(|stored| !check.replaces(&stored)) {
				return Ok(());
			}
			tx.execute(
				"INSERT INTO stack_record (stack_id, record) VALUES (?1, ?2)
				 ON CONFLICT (stack_id) DO UPDATE SET record = excluded.record",
				rusqlite::params![stack_id, record],
			)?;
			Ok(())
		})
		.await
		.map_err(|error| error.to_string().into())
	}
}

/// The record stored for `stack_id`, read in the caller's transaction.
#[cfg(not(target_arch = "wasm32"))]
fn stored_record(conn: &rusqlite::Connection, stack_id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
	use rusqlite::OptionalExtension;

	conn.query_row("SELECT record FROM stack_record WHERE stack_id = ?1", [stack_id], |row| {
		row.get(0)
	})
	.optional()
}

#[cfg(test)]
mod tests;
