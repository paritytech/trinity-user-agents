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
