use super::*;
use std::{collections::HashMap, sync::Mutex};

use crate::test_support::{sso_session_info, test_run};

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

/// The run the chains are in.
fn this_run() -> StackRun {
	test_run([0xcc; 32])
}

/// The run before a wipe replaced Asset Hub and made it [`this_run`].
fn earlier_run() -> StackRun {
	test_run([0x99; 32])
}

/// What is stored for `session`, whichever run stored it.
fn stored(storage: &MemStorage, session: &SessionInfo) -> Option<Vec<u8>> {
	futures::executor::block_on(storage.read_core_storage(storage_key(session).unwrap())).unwrap()
}

#[test]
fn entries_stored_with_string_ids_read_and_write_unchanged() {
	// A stored entry is the product's id string, the resource and the key. A
	// change to that layout would stop the keys an install holds from decoding.
	let stored =
		vec![("dotli.dot".to_string(), AllowanceResource::StatementStore, vec![2u8; 64])].encode();
	let entries = decode_entries(&stored).expect("string-keyed entries decode");
	assert_eq!(entries.encode(), stored);
}

#[test]
fn stores_allowance_keys_by_product_and_resource() {
	let storage = MemStorage::default();
	let session = sso_session_info();
	let run = this_run();

	futures::executor::block_on(async {
		write_allowance_key(
			&storage,
			&run,
			&session,
			&ProductId::for_test("dotli.dot"),
			AllowanceResource::Bulletin,
			vec![1; 64],
		)
		.await
		.unwrap();
		write_allowance_key(
			&storage,
			&run,
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
				&run,
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
				&run,
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
				&run,
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
	let run = this_run();

	futures::executor::block_on(async {
		write_allowance_key(
			&storage,
			&run,
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
				&run,
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

/// Writes a Bulletin key of `byte`s for `product` in `run`.
fn write_bulletin_key(storage: &MemStorage, run: &StackRun, product: &str, byte: u8) {
	futures::executor::block_on(write_allowance_key(
		storage,
		run,
		&sso_session_info(),
		&ProductId::for_test(product),
		AllowanceResource::Bulletin,
		vec![byte; 64],
	))
	.unwrap()
}

/// The Bulletin key `run` reads for `product`.
fn read_bulletin_key(storage: &MemStorage, run: &StackRun, product: &str) -> Option<Vec<u8>> {
	futures::executor::block_on(read_allowance_key(
		storage,
		run,
		&sso_session_info(),
		&ProductId::for_test(product),
		AllowanceResource::Bulletin,
	))
	.unwrap()
}

/// A Bulletin key of `byte`s for `product`, as an entry of the stored list.
fn bulletin_entry(product: &str, byte: u8) -> StoredAllowanceEntry {
	StoredAllowanceEntry {
		product_id: ProductId::for_test(product),
		resource: AllowanceResource::Bulletin,
		slot_account_key: vec![byte; 64],
	}
}

/// Storage holding the keys of `dotli.dot` and `other.dot` as stored in `run`,
/// or as a bare list when there is none.
fn storage_with_two_keys(run: Option<&StackRun>) -> MemStorage {
	let storage = MemStorage::default();
	let entries = vec![bulletin_entry("dotli.dot", 1), bulletin_entry("other.dot", 2)].encode();
	let stored = run.map_or(entries.clone(), |run| run.stamp(&entries));
	futures::executor::block_on(
		storage.write_core_storage(storage_key(&sso_session_info()).unwrap(), stored),
	)
	.unwrap();
	storage
}

/// An allowance key names a slot account on a chain, and a wipe replaces the
/// chains. A key stored in another run must not be handed out, so that the
/// product asks for a new allowance instead of one the chain no longer knows.
/// A list stored without a run could be from any, so it is not read either.
#[test]
fn allowance_keys_are_read_only_in_the_run_that_stored_them() {
	let stored_earlier = storage_with_two_keys(Some(&earlier_run()));
	let stored_without_a_run = storage_with_two_keys(None);

	assert_eq!(
		[
			read_bulletin_key(&stored_earlier, &earlier_run(), "dotli.dot"),
			read_bulletin_key(&stored_earlier, &this_run(), "dotli.dot"),
			read_bulletin_key(&stored_without_a_run, &this_run(), "dotli.dot"),
		],
		[Some(vec![1; 64]), None, None]
	);
}

/// The first key written after a wipe starts the list again, so the keys of
/// the run before are gone from storage rather than kept beside it.
#[test]
fn a_key_written_after_a_wipe_replaces_the_keys_stored_before_it() {
	let storage = storage_with_two_keys(Some(&earlier_run()));
	write_bulletin_key(&storage, &this_run(), "other.dot", 3);

	assert_eq!(
		stored(&storage, &sso_session_info()),
		Some(this_run().stamp(&vec![bulletin_entry("other.dot", 3)].encode()))
	);
}

/// Revoking a product removes its key for good. In a list of this run that
/// leaves the other products' keys. A list this run does not read, stored in
/// another run or without one, is cleared whole, so the revoked key cannot
/// come back and no secret stays at rest.
#[test]
fn revoking_a_product_keeps_only_the_other_keys_of_this_run() {
	let storages = [
		storage_with_two_keys(Some(&this_run())),
		storage_with_two_keys(Some(&earlier_run())),
		storage_with_two_keys(None),
	];

	let after_revoke = storages.map(|storage| {
		futures::executor::block_on(clear_product_allowance_keys(
			&storage,
			&this_run(),
			&sso_session_info(),
			&ProductId::for_test("dotli.dot"),
		))
		.unwrap();
		stored(&storage, &sso_session_info())
	});

	let only_the_other_key = this_run().stamp(&vec![bulletin_entry("other.dot", 2)].encode());
	assert_eq!(after_revoke, [Some(only_the_other_key), None, None]);
}
