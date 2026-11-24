use super::*;
use crate::{
	stack::signed_pointer,
	test_support::{
		MemoryCoreStorage, on_testnet, published_test_descriptor_at, sign_test_descriptor,
		stored_test_record, without_bulletin,
	},
};

/// The record a reader stores for the test stack's descriptor at `sequence`.
fn record_at(sequence: u32) -> Vec<u8> {
	stored_test_record(&published_test_descriptor_at(sequence), 0)
}

/// The rules of a build that bundles the test stack at `sequence` 1.
fn bundled_rules() -> CopyRules {
	let (link, bundled) = sign_test_descriptor(&published_test_descriptor_at(1));
	let pointer = signed_pointer(&link).expect("test link carries a key");
	CopyRules::new(pointer, &bundled, false).expect("bundled copy is valid")
}

/// Two runtimes can refresh at once while a host rebuilds its runtime. The
/// slower one must not put an older copy back in the browser's core storage,
/// or the next start would go back to chains that no longer exist.
#[test]
fn an_older_record_does_not_replace_a_newer_one_in_core_storage() {
	let rules = bundled_rules();
	let stack_id = rules.pointer().stack_id();
	let records = CoreStorageRecords::new(Arc::new(MemoryCoreStorage::default()));
	let write = |sequence| {
		let record = record_at(sequence);
		let check = ReplaceCheck::new(rules.clone(), &record);
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

	assert_eq!([after_an_older_one, stored()], [Some(record_at(3)), Some(record_at(4))]);
}

/// The core never starts from a record whose copy fails its rules, so that
/// record must not keep a copy the core took out of the store. A record that
/// passes still keeps an older copy out.
#[test]
fn only_a_record_whose_copy_passes_the_cores_rules_keeps_an_older_copy_out() {
	let check = ReplaceCheck::new(bundled_rules(), &record_at(3));
	let existing = [
		record_at(5),
		record_at(3),
		stored_test_record(&on_testnet(published_test_descriptor_at(5)), 0),
		stored_test_record(&without_bulletin(published_test_descriptor_at(5)), 0),
		b"not a record".to_vec(),
	];

	assert_eq!(existing.map(|existing| check.replaces(&existing)), [false, true, true, true, true]);
}

/// The core database on iOS and Android applies the same check, in one
/// transaction: a record on another TLD is replaced whatever its `sequence`,
/// and one the core can run keeps an older one out.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_core_database_replaces_an_ignored_record_and_keeps_a_newer_one_it_can_run() {
	let (_directory, db) = crate::store::open_temporary_core_db();
	let rules = bundled_rules();
	let stack_id = rules.pointer().stack_id();
	let write = |record: Vec<u8>| {
		let check = ReplaceCheck::new(rules.clone(), &record);
		futures::executor::block_on(db.write_stack_record(&stack_id, record, check))
			.expect("the database writes");
	};
	let stored = || {
		futures::executor::block_on(db.read_stack_record(&stack_id)).expect("the database reads")
	};

	write(stored_test_record(&on_testnet(published_test_descriptor_at(5)), 0));
	write(record_at(3));
	let over_an_ignored_one = stored();
	write(record_at(5));
	write(record_at(4));

	assert_eq!([over_an_ignored_one, stored()], [Some(record_at(3)), Some(record_at(5))]);
}
