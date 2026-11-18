use std::{collections::HashMap, sync::Mutex};

use parity_scale_codec::Encode;
use trinity_stack::{Refresh, Served, StackReader};

use super::*;
use crate::{
	stack::signed_pointer,
	test_support::{TEST_STACK_PEOPLE, published_test_descriptor, sign_test_descriptor},
};

/// Core storage held in memory, as a browser host's might be.
#[derive(Default)]
struct MemoryCoreStorage(Mutex<HashMap<Vec<u8>, Vec<u8>>>);

#[async_trait::async_trait]
impl CoreStorage for MemoryCoreStorage {
	async fn read_core_storage(
		&self,
		key: StackStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		Ok(self.0.lock().expect("storage mutex poisoned").get(&key.encode()).cloned())
	}

	async fn write_core_storage(
		&self,
		key: StackStorageKey,
		value: Vec<u8>,
	) -> Result<(), GenericError> {
		self.0.lock().expect("storage mutex poisoned").insert(key.encode(), value);
		Ok(())
	}

	async fn clear_core_storage(&self, key: StackStorageKey) -> Result<(), GenericError> {
		self.0.lock().expect("storage mutex poisoned").remove(&key.encode());
		Ok(())
	}
}

/// The record a reader stores for the test stack's descriptor at `sequence`.
fn record_at(pointer: &Pointer, sequence: u32) -> Vec<u8> {
	let (_, bytes) =
		sign_test_descriptor(&published_test_descriptor(sequence, "first", TEST_STACK_PEOPLE));
	StackReader::new(Refresh::Never)
		.fetched(pointer, Served::Document { bytes: &bytes, etag: None }, 0)
		.expect("the copy passes the descriptor rules")
		.stored
		.expect("a first copy is stored")
}

/// Two runtimes can refresh at once while a host rebuilds its runtime. The
/// slower one must not put an older copy back in the browser's core storage,
/// or the next start would go back to chains that no longer exist.
#[test]
fn an_older_record_does_not_replace_a_newer_one_in_core_storage() {
	let (link, _) = sign_test_descriptor(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE));
	let pointer = signed_pointer(&link).expect("test link carries a key");
	let stack_id = pointer.stack_id();
	let records = CoreStorageRecords::new(Arc::new(MemoryCoreStorage::default()));
	let write = |sequence| {
		let record = record_at(&pointer, sequence);
		let check = ReplaceCheck::new(pointer.clone(), &record);
		futures::executor::block_on(records.write_stack_record(&stack_id, record, check))
			.expect("memory storage writes");
	};
	let stored = || {
		futures::executor::block_on(records.read_stack_record(&stack_id))
			.expect("memory storage reads")
	};

	write(3);
	write(2);
	let after_an_older_one = stored();
	write(4);

	assert_eq!(
		[after_an_older_one, stored()],
		[Some(record_at(&pointer, 3)), Some(record_at(&pointer, 4))]
	);
}
