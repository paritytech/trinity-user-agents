//! Core storage held in memory, for tests that read back what the core wrote.

use std::{collections::HashMap, sync::Mutex};

use parity_scale_codec::Encode;
use truapi::latest::GenericError;

use crate::platform::{CoreStorage, CoreStorageKey};

/// Core storage held in memory, as a browser host's might be.
#[derive(Default)]
pub struct MemoryCoreStorage(Mutex<HashMap<Vec<u8>, Vec<u8>>>);

#[async_trait::async_trait]
impl CoreStorage for MemoryCoreStorage {
	async fn read_core_storage(
		&self,
		key: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		Ok(self.0.lock().expect("storage mutex poisoned").get(&key.encode()).cloned())
	}

	async fn write_core_storage(
		&self,
		key: CoreStorageKey,
		value: Vec<u8>,
	) -> Result<(), GenericError> {
		self.0.lock().expect("storage mutex poisoned").insert(key.encode(), value);
		Ok(())
	}

	async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
		self.0.lock().expect("storage mutex poisoned").remove(&key.encode());
		Ok(())
	}
}
