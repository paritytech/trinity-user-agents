use super::*;
use std::{collections::HashMap, sync::Mutex};

use crate::test_support::sso_session_info;

#[derive(Default)]
struct MemStorage {
	inner: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
}

#[async_trait::async_trait]
impl CoreStorage for MemStorage {
	async fn read_core_storage(
		&self,
		key: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, GenericError> {
		Ok(self.inner.lock().expect("storage mutex poisoned").get(&key.encode()).cloned())
	}

	async fn write_core_storage(
		&self,
		key: CoreStorageKey,
		value: Vec<u8>,
	) -> Result<(), GenericError> {
		self.inner.lock().expect("storage mutex poisoned").insert(key.encode(), value);
		Ok(())
	}

	async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
		self.inner.lock().expect("storage mutex poisoned").remove(&key.encode());
		Ok(())
	}
}

#[test]
fn entries_stored_with_string_ids_read_and_write_unchanged() {
	// Installs already hold entries keyed by the id string, so the stored
	// format must stay that string.
	let stored =
		vec![("dotli.dot".to_string(), AllowanceResource::StatementStore, vec![2u8; 64])].encode();
	let entries = decode_entries(&stored).expect("string-keyed entries decode");
	assert_eq!(encode_entries(entries), stored);
}

#[test]
fn stores_allowance_keys_by_product_and_resource() {
	let storage = MemStorage::default();
	let session = sso_session_info();

	futures::executor::block_on(async {
		write_allowance_key(
			&storage,
			&session,
			&ProductId::for_test("dotli.dot"),
			AllowanceResource::Bulletin,
			vec![1; 64],
		)
		.await
		.unwrap();
		write_allowance_key(
			&storage,
			&session,
			&ProductId::for_test("dotli.dot"),
			AllowanceResource::StatementStore,
			vec![2; 64],
		)
		.await
		.unwrap();

		assert_eq!(
			read_allowance_key(
				&storage,
				&session,
				&ProductId::for_test("dotli.dot"),
				AllowanceResource::Bulletin
			)
			.await
			.unwrap(),
			Some(vec![1; 64])
		);
		assert_eq!(
			read_allowance_key(
				&storage,
				&session,
				&ProductId::for_test("dotli.dot"),
				AllowanceResource::StatementStore
			)
			.await
			.unwrap(),
			Some(vec![2; 64])
		);
		assert_eq!(
			read_allowance_key(
				&storage,
				&session,
				&ProductId::for_test("other.dot"),
				AllowanceResource::Bulletin
			)
			.await
			.unwrap(),
			None
		);
	});
}

#[test]
fn clears_session_allowance_keys() {
	let storage = MemStorage::default();
	let session = sso_session_info();

	futures::executor::block_on(async {
		write_allowance_key(
			&storage,
			&session,
			&ProductId::for_test("dotli.dot"),
			AllowanceResource::Bulletin,
			vec![1; 64],
		)
		.await
		.unwrap();
		clear_session_allowance_keys(&storage, &session).await.unwrap();
		assert_eq!(
			read_allowance_key(
				&storage,
				&session,
				&ProductId::for_test("dotli.dot"),
				AllowanceResource::Bulletin
			)
			.await
			.unwrap(),
			None
		);
	});
}
