//! Where the core keeps the last descriptor it verified for a stack, so the
//! next start does not go back to the bundled copy.

use truapi::latest::GenericError;

/// The store of each stack's descriptor record. A record is opaque to the
/// store.
#[async_trait::async_trait]
pub trait StackRecords: Send + Sync {
	/// The record stored for `stack_id`, or `None` when there is none.
	///
	/// Called once, while the runtime is built, so it must answer without
	/// waiting on the host: a native host builds the runtime on its main
	/// thread.
	fn read_stack_record(&self, stack_id: &str) -> Result<Option<Vec<u8>>, GenericError>;

	/// Stores `record` for `stack_id`, replacing any earlier one.
	async fn write_stack_record(&self, stack_id: &str, record: Vec<u8>)
	-> Result<(), GenericError>;
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl StackRecords for crate::store::Db {
	fn read_stack_record(&self, stack_id: &str) -> Result<Option<Vec<u8>>, GenericError> {
		use rusqlite::OptionalExtension;

		let stack_id = stack_id.to_string();
		futures::executor::block_on(self.read(move |conn| {
			Ok(conn
				.query_row(
					"SELECT record FROM stack_record WHERE stack_id = ?1",
					[stack_id],
					|row| row.get(0),
				)
				.optional()?)
		}))
		.map_err(|error| error.to_string().into())
	}

	async fn write_stack_record(
		&self,
		stack_id: &str,
		record: Vec<u8>,
	) -> Result<(), GenericError> {
		let stack_id = stack_id.to_string();
		self.write(move |tx| {
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
