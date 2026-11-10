//! The core's storage for the stack it runs.

use truapi::latest::GenericError;

use crate::platform::{CoreStorage, CoreStorageKey, StackStorageKey};

/// Host storage for one stack. It adds the stack's id to every key, so stacks
/// do not share entries.
///
/// Built by [`Stack::storage`](super::Stack::storage). The core stores nothing
/// except through it.
pub struct StackStorage<'a, S: CoreStorage + ?Sized> {
	host: &'a S,
	stack: &'a str,
}

impl<'a, S: CoreStorage + ?Sized> StackStorage<'a, S> {
	/// Storage in `host` for the stack with the id `stack`.
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
