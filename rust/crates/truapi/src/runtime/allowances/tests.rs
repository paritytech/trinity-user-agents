use super::*;
use crate::test_support::sso_session_info;

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
	let storage = crate::test_support::MemoryCoreStorage::default();
	let session = sso_session_info();

	futures::executor::block_on(async {
		write_allowance_key(
			&crate::test_support::test_storage(&storage),
			&session,
			&ProductId::for_test("dotli.dot"),
			AllowanceResource::Bulletin,
			vec![1; 64],
		)
		.await
		.unwrap();
		write_allowance_key(
			&crate::test_support::test_storage(&storage),
			&session,
			&ProductId::for_test("dotli.dot"),
			AllowanceResource::StatementStore,
			vec![2; 64],
		)
		.await
		.unwrap();

		assert_eq!(
			read_allowance_key(
				&crate::test_support::test_storage(&storage),
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
				&crate::test_support::test_storage(&storage),
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
				&crate::test_support::test_storage(&storage),
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
	let storage = crate::test_support::MemoryCoreStorage::default();
	let session = sso_session_info();

	futures::executor::block_on(async {
		write_allowance_key(
			&crate::test_support::test_storage(&storage),
			&session,
			&ProductId::for_test("dotli.dot"),
			AllowanceResource::Bulletin,
			vec![1; 64],
		)
		.await
		.unwrap();
		clear_session_allowance_keys(&crate::test_support::test_storage(&storage), &session)
			.await
			.unwrap();
		assert_eq!(
			read_allowance_key(
				&crate::test_support::test_storage(&storage),
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
