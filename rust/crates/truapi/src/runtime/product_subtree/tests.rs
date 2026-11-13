use super::*;
use crate::test_support::{MemoryCoreStorage, sso_session_info, test_storage, test_storage_key};

#[test]
fn each_product_keeps_its_own_slot() {
	let storage = MemoryCoreStorage::default();
	let session = sso_session_info();

	futures::executor::block_on(async {
		write_product_subtree(
			&test_storage(&storage),
			&session,
			&ProductId::for_test("one.dot"),
			[1; 32],
		)
		.await
		.unwrap();
		write_product_subtree(
			&test_storage(&storage),
			&session,
			&ProductId::for_test("two.dot"),
			[2; 32],
		)
		.await
		.unwrap();
		write_product_subtree(
			&test_storage(&storage),
			&session,
			&ProductId::for_test("one.dot"),
			[3; 32],
		)
		.await
		.unwrap();

		assert_eq!(
			read_product_subtree(
				&test_storage(&storage),
				&session,
				&ProductId::for_test("one.dot")
			)
			.await
			.unwrap(),
			Some([3; 32])
		);
		assert_eq!(
			read_product_subtree(
				&test_storage(&storage),
				&session,
				&ProductId::for_test("two.dot")
			)
			.await
			.unwrap(),
			Some([2; 32])
		);
		assert_eq!(
			read_product_subtree(
				&test_storage(&storage),
				&session,
				&ProductId::for_test("absent.dot")
			)
			.await
			.unwrap(),
			None
		);
	});
}

#[test]
fn the_value_is_the_bare_key_a_host_can_read() {
	let storage = MemoryCoreStorage::default();
	let session = sso_session_info();

	futures::executor::block_on(async {
		write_product_subtree(
			&test_storage(&storage),
			&session,
			&ProductId::for_test("one.dot"),
			[7; 32],
		)
		.await
		.unwrap();

		// Hosts derive addresses straight from this slot, so the value must
		// stay 32 unframed bytes rather than anything this module encodes.
		let raw = storage
			.read_core_storage(test_storage_key(
				product_subtree_key(&session, &ProductId::for_test("one.dot")).unwrap(),
			))
			.await
			.unwrap();
		assert_eq!(raw, Some([7u8; 32].to_vec()));
	});
}

#[test]
fn removing_one_product_leaves_the_others() {
	let storage = MemoryCoreStorage::default();
	let session = sso_session_info();

	futures::executor::block_on(async {
		write_product_subtree(
			&test_storage(&storage),
			&session,
			&ProductId::for_test("one.dot"),
			[1; 32],
		)
		.await
		.unwrap();
		write_product_subtree(
			&test_storage(&storage),
			&session,
			&ProductId::for_test("two.dot"),
			[2; 32],
		)
		.await
		.unwrap();

		// Backs the rollback in `persist_product_subtree_if_current`.
		remove_product_subtree(&test_storage(&storage), &session, &ProductId::for_test("one.dot"))
			.await
			.unwrap();

		assert_eq!(
			read_product_subtree(
				&test_storage(&storage),
				&session,
				&ProductId::for_test("one.dot")
			)
			.await
			.unwrap(),
			None
		);
		assert_eq!(
			read_product_subtree(
				&test_storage(&storage),
				&session,
				&ProductId::for_test("two.dot")
			)
			.await
			.unwrap(),
			Some([2; 32])
		);
	});
}

#[test]
fn a_wrong_length_slot_reads_as_empty() {
	let storage = MemoryCoreStorage::default();
	let session = sso_session_info();

	futures::executor::block_on(async {
		storage
			.write_core_storage(
				test_storage_key(
					product_subtree_key(&session, &ProductId::for_test("one.dot")).unwrap(),
				),
				vec![0xff; 31],
			)
			.await
			.unwrap();

		assert_eq!(
			read_product_subtree(
				&test_storage(&storage),
				&session,
				&ProductId::for_test("one.dot")
			)
			.await
			.unwrap(),
			None
		);
	});
}
