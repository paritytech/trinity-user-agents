//! The core's storage for the stack it runs.

use truapi::latest::GenericError;

use crate::platform::{CoreStorage, CoreStorageKey, StackStorageKey};

/// Host storage narrowed to one stack: every slot is read and written under
/// that stack's identity.
///
/// Built by [`Stack::storage`](super::Stack::storage). The core stores nothing
/// except through it, so nothing it stored for one stack is visible while it
/// runs another.
pub struct StackStorage<'a, S: CoreStorage + ?Sized> {
	host: &'a S,
	stack: &'a str,
}

impl<'a, S: CoreStorage + ?Sized> StackStorage<'a, S> {
	/// The slots of the stack identified by `stack`, kept in `host`.
	pub fn new(host: &'a S, stack: &'a str) -> Self {
		Self { host, stack }
	}

	/// Read the value in `slot`.
	pub async fn read(&self, slot: CoreStorageKey) -> Result<Option<Vec<u8>>, GenericError> {
		self.host.read_core_storage(self.key(slot)).await
	}

	/// Write `value` to `slot`.
	pub async fn write(&self, slot: CoreStorageKey, value: Vec<u8>) -> Result<(), GenericError> {
		self.host.write_core_storage(self.key(slot), value).await
	}

	/// Clear `slot`.
	pub async fn clear(&self, slot: CoreStorageKey) -> Result<(), GenericError> {
		self.host.clear_core_storage(self.key(slot)).await
	}

	fn key(&self, slot: CoreStorageKey) -> StackStorageKey {
		StackStorageKey { stack: self.stack.to_string(), slot }
	}
}
