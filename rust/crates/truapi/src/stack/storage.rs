//! The core's storage for the stack it runs.

use truapi::latest::GenericError;

use super::Stack;
use crate::platform::{CoreStorage, CoreStorageKey};

/// Host storage as the core reaches it for the stack it runs.
///
/// [`Stack::storage`] is its only constructor.
pub struct StackStorage<'a, S: CoreStorage + ?Sized> {
	host: &'a S,
}

impl Stack {
	/// The core's storage for this stack, kept in `host`.
	pub fn storage<'a, S: CoreStorage + ?Sized>(&'a self, host: &'a S) -> StackStorage<'a, S> {
		StackStorage { host }
	}
}

impl<S: CoreStorage + ?Sized> StackStorage<'_, S> {
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
