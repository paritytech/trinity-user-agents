//! The core's storage for the stack it runs.

use truapi::latest::GenericError;

use crate::platform::{CoreStorage, CoreStorageKey};

/// Host storage as the core reaches it for the stack it runs.
///
/// Built by [`Stack::storage`](super::Stack::storage). The core stores nothing
/// except through it.
pub struct StackStorage<'a, S: CoreStorage + ?Sized> {
	host: &'a S,
}

impl<'a, S: CoreStorage + ?Sized> StackStorage<'a, S> {
	/// The stack's slots, kept in `host`.
	pub fn new(host: &'a S) -> Self {
		Self { host }
	}

	/// Read the value in `slot`.
	pub async fn read(&self, slot: CoreStorageKey) -> Result<Option<Vec<u8>>, GenericError> {
		self.host.read_core_storage(slot).await
	}

	/// Write `value` to `slot`.
	pub async fn write(&self, slot: CoreStorageKey, value: Vec<u8>) -> Result<(), GenericError> {
		self.host.write_core_storage(slot, value).await
	}

	/// Clear `slot`.
	pub async fn clear(&self, slot: CoreStorageKey) -> Result<(), GenericError> {
		self.host.clear_core_storage(slot).await
	}
}
