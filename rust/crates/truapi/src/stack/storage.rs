//! The core's storage for the stack it runs.

use tracing::warn;
use truapi::latest::GenericError;

use crate::platform::{CoreStorage, CoreStorageKey, StackStorageKey};

/// Host storage for one stack. It adds the stack's id to every key, so stacks
/// do not share entries.
///
/// A value the host holds under a slot alone, with no stack id, belongs to the
/// first stack that reads the slot: [`Self::read`] moves it under that stack's
/// key. Every miss therefore costs a second host read, and every clear a
/// second host clear.
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
	///
	/// A value found only under the slot alone is written under the stack's
	/// key and then cleared from under the slot. If that write fails the
	/// value is still returned and stays where it was, so the next read moves
	/// it.
	pub async fn read(&self, slot: CoreStorageKey) -> Result<Option<Vec<u8>>, GenericError> {
		if let Some(value) = self.host.read_core_storage(self.key(slot.clone())).await? {
			return Ok(Some(value));
		}
		let Some(value) = self.host.read_unscoped_core_storage(slot.clone()).await? else {
			return Ok(None);
		};
		// The stack's copy is written before the other is cleared: a kill
		// between the two leaves both, and the stack's is the one read.
		match self.host.write_core_storage(self.key(slot.clone()), value.clone()).await {
			Ok(()) => {
				if let Err(error) = self.host.clear_unscoped_core_storage(slot).await {
					warn!(reason = %error.reason, "clearing a moved core storage value failed");
				}
			},
			Err(error) => warn!(reason = %error.reason, "moving a core storage value failed"),
		}
		Ok(Some(value))
	}

	/// Write `value` to `slot`.
	pub async fn write(&self, slot: CoreStorageKey, value: Vec<u8>) -> Result<(), GenericError> {
		self.host.write_core_storage(self.key(slot), value).await
	}

	/// Clear `slot`, under the stack's key and under the slot alone.
	///
	/// The value under the slot alone goes first. Cleared second, a kill
	/// between the two would leave it for the next read to bring back.
	pub async fn clear(&self, slot: CoreStorageKey) -> Result<(), GenericError> {
		self.host.clear_unscoped_core_storage(slot.clone()).await?;
		self.host.clear_core_storage(self.key(slot)).await
	}

	fn key(&self, slot: CoreStorageKey) -> StackStorageKey {
		StackStorageKey { stack: self.stack.to_string(), slot }
	}
}
