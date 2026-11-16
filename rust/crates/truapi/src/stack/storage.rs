//! The core's storage for the stack it runs.

use std::sync::Arc;

use futures::lock::Mutex;
use parity_scale_codec::Encode;
use truapi::latest::GenericError;

use super::Stack;
use crate::platform::{CoreStorage, CoreStorageKey, StackStorageKey};

/// Host storage for one stack. It adds the stack's id to every key, so stacks
/// do not share entries.
///
/// The values a host stored under a slot alone, before keys included a stack
/// id, belong to one stack: the first that looks for one, which records its id
/// under [`CoreStorageKey::UnscopedEntriesOwner`]. For that stack a slot with
/// no value under its key reads the value under the slot alone, and a clear
/// removes both. Other stacks never see those values. Two hosts that look at
/// the same moment can both take them.
///
/// Built by [`Stack::storage`]. The core stores nothing except through it.
pub struct StackStorage<'a, S: CoreStorage + ?Sized> {
	host: &'a S,
	stack: &'a str,
	holds_unscoped: Arc<Mutex<Option<bool>>>,
}

impl Stack {
	/// The core's storage for this stack over the host's `storage`.
	///
	/// Every storage built from a stack, or a clone of it, shares whether the
	/// stack holds the values stored under a slot alone, so the host is asked
	/// once a run.
	pub fn storage<'a, S: CoreStorage + ?Sized>(&'a self, storage: &'a S) -> StackStorage<'a, S> {
		StackStorage {
			host: storage,
			stack: &self.identity.id,
			holds_unscoped: self.holds_unscoped.clone(),
		}
	}
}

impl<'a, S: CoreStorage + ?Sized> StackStorage<'a, S> {
	/// Storage in `host` for the stack with the id `stack`, which asks the
	/// host on its own whether the stack holds the values stored under a slot
	/// alone.
	pub fn new(host: &'a S, stack: &'a str) -> Self {
		Self { host, stack, holds_unscoped: Arc::default() }
	}

	/// Read the value in `slot`.
	pub async fn read(&self, slot: CoreStorageKey) -> Result<Option<Vec<u8>>, GenericError> {
		if let Some(value) = self.host.read_core_storage(self.key(slot.clone())).await? {
			return Ok(Some(value));
		}
		if !self.holds_unscoped().await? {
			return Ok(None);
		}
		self.host.read_unscoped_core_storage(slot).await
	}

	/// Write `value` to `slot`.
	pub async fn write(&self, slot: CoreStorageKey, value: Vec<u8>) -> Result<(), GenericError> {
		self.host.write_core_storage(self.key(slot), value).await
	}

	/// Clear `slot`, under the stack's key and, for the stack that holds them,
	/// under the slot alone.
	///
	/// The value under the slot alone goes first. Cleared second, a kill
	/// between the two would leave it for the next read to bring back.
	pub async fn clear(&self, slot: CoreStorageKey) -> Result<(), GenericError> {
		if self.holds_unscoped().await? {
			self.host.clear_unscoped_core_storage(slot.clone()).await?;
		}
		self.host.clear_core_storage(self.key(slot)).await
	}

	/// Whether this stack holds the values stored under a slot alone. The
	/// first stack to ask records its id; every later one reads it.
	async fn holds_unscoped(&self) -> Result<bool, GenericError> {
		let mut holds = self.holds_unscoped.lock().await;
		if let Some(holds) = *holds {
			return Ok(holds);
		}
		let owner = CoreStorageKey::UnscopedEntriesOwner;
		let stack = self.stack.encode();
		let mut recorded = self.host.read_unscoped_core_storage(owner.clone()).await?;
		if recorded.is_none() {
			self.host.write_unscoped_core_storage(owner.clone(), stack.clone()).await?;
			// Read back: a host that keeps nothing under a slot alone drops the
			// write, and has no values for this stack to take.
			recorded = self.host.read_unscoped_core_storage(owner).await?;
		}
		let held = recorded == Some(stack);
		*holds = Some(held);
		Ok(held)
	}

	fn key(&self, slot: CoreStorageKey) -> StackStorageKey {
		StackStorageKey { stack: self.stack.to_string(), slot }
	}
}
