use super::*;
use std::{collections::HashMap, sync::Mutex};

use crate::{
	platform::Stack,
	test_support::{paseo_test_stack, sso_session_info, test_stack},
};

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
	// An entry names its product by the id string, so the stored format must
	// stay that string.
	let stored =
		vec![("dotli.dot".to_string(), AllowanceResource::StatementStore, vec![2u8; 64])].encode();
	let entries = decode_entries(&stored).expect("string-keyed entries decode");
	assert_eq!(entries.encode(), stored);
}

#[test]
fn stores_allowance_keys_by_product_and_resource() {
	let storage = MemStorage::default();
	let session = sso_session_info();

	futures::executor::block_on(async {
		write_allowance_key(
			&storage,
			&paseo_test_stack().run(),
			&session,
			&ProductId::for_test("dotli.dot"),
			AllowanceResource::Bulletin,
			vec![1; 64],
		)
		.await
		.unwrap();
		write_allowance_key(
			&storage,
			&paseo_test_stack().run(),
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
				&paseo_test_stack().run(),
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
				&paseo_test_stack().run(),
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
				&paseo_test_stack().run(),
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
			&paseo_test_stack().run(),
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
				&paseo_test_stack().run(),
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

/// An install that stored allowance keys before they carried a run holds
/// them as a bare list. Which run they belong to is unknown, so none is handed
/// out; revoking a product clears them, so its secret does not stay at rest.
#[test]
fn allowance_keys_stored_without_a_run_are_not_read_and_are_cleared_on_revoke() {
	let storage = MemStorage::default();
	let session = sso_session_info();
	let product = ProductId::for_test("dotli.dot");
	let run = paseo_test_stack().run();
	let unstamped = vec![StoredAllowanceEntry {
		product_id: product.clone(),
		resource: AllowanceResource::Bulletin,
		slot_account_key: vec![1; 64],
	}]
	.encode();
	let key = storage_key(&session).unwrap();

	let (read, after_revoke) = futures::executor::block_on(async {
		storage.write_core_storage(key.clone(), unstamped).await.unwrap();
		let read =
			read_allowance_key(&storage, &run, &session, &product, AllowanceResource::Bulletin)
				.await
				.unwrap();
		clear_product_allowance_keys(&storage, &run, &session, &product).await.unwrap();
		(read, storage.read_core_storage(key).await.unwrap())
	});

	assert_eq!((read, after_revoke), (None, None));
}

/// An allowance key names a slot account on a chain, and a wipe replaces the
/// chains. A key stored in another run must not be handed out, so that the
/// product asks for a new allowance instead of one the chain no longer knows.
#[test]
fn allowance_keys_stored_in_another_run_of_the_chains_are_not_read() {
	let storage = MemStorage::default();
	let session = sso_session_info();
	let product = ProductId::for_test("dotli.dot");
	let read = |stack: Stack| {
		futures::executor::block_on(read_allowance_key(
			&storage,
			&stack.run(),
			&session,
			&product,
			AllowanceResource::Bulletin,
		))
		.unwrap()
	};

	futures::executor::block_on(write_allowance_key(
		&storage,
		&paseo_test_stack().run(),
		&session,
		&product,
		AllowanceResource::Bulletin,
		vec![1; 64],
	))
	.unwrap();

	let wiped = test_stack("paseo", [0x77; 32], [0xbb; 32], [0xcc; 32]);
	assert_eq!([read(paseo_test_stack()), read(wiped)], [Some(vec![1; 64]), None]);
}
