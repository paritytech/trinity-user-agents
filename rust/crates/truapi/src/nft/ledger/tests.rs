use std::{collections::HashSet, num::NonZeroU32, sync::Arc};

use futures::executor::block_on;
use tempfile::TempDir;
use truapi::latest::GenericError;

use super::*;
use crate::{
	host_logic::product_account::derive_sr25519_hard_path,
	installation::InstallationId,
	nft::{
		ItemSlot, UnknownPurse, item_keys_from_entropy,
		keys::ItemIndex,
		testing::{
			FixedInstallation, TEST_ENTROPY, TEST_INSTALLATION, insert_item_row, test_ledger_deps,
		},
	},
	platform::async_trait,
	runtime::AuthorityError,
	store::open_temporary_core_db,
};

/// A ledger over a fresh database with one product purse, id 0.
fn ledger_with_purse(deps: impl FnOnce(Db) -> NftLedgerDeps) -> (TempDir, NftLedger) {
	let (dir, db) = open_temporary_core_db();
	let ledger = NftLedger::new(deps(db));
	block_on(ledger.create_purse("tickets".to_owned(), PurseTag(b"tickets.dot".to_vec())))
		.expect("purse is created");
	(dir, ledger)
}

fn allocate(ledger: &NftLedger, count: u32) -> Result<Vec<ItemKey>, AllocateItemsError> {
	let count = NonZeroU32::new(count).expect("count is positive");
	block_on(ledger.allocate_items(PurseId(0), count))
}

/// The keys of purse 0 under the test installation at `indices`.
fn expected_keys(indices: std::ops::Range<u32>) -> Vec<ItemKey> {
	let slot = |index| ItemSlot {
		purse: PurseId(0),
		installation: TEST_INSTALLATION,
		index: ItemIndex(index),
	};
	let slots: Vec<ItemSlot> = indices.map(slot).collect();
	item_keys_from_entropy(&TEST_ENTROPY, &slots).expect("test entropy derives")
}

/// Recovery derives these keys again from the entropy alone, so the path
/// and its encoding must never change.
#[test]
fn item_keys_follow_the_hard_path_nft_purse_installation_index() {
	let installation = format!("0x{}", "1d".repeat(32));
	let by_path = derive_sr25519_hard_path(&TEST_ENTROPY, &["nft", "0", &installation, "3"])
		.expect("path derives");
	let [_, _, _, by_slot] = expected_keys(0..4)[..] else { panic!("four keys") };
	let pinned = "d837c9c558b3d5c58df68608ba64f5db6a546271368c1c386efee373b33f4636";
	let by_slot_hex = hex::encode(by_slot.0);
	let by_path_key = by_path.public.to_bytes();
	assert_eq!((by_slot_hex, by_slot.0), (pinned.to_owned(), by_path_key));
}

/// Recovery scans indices from 0, so a purse's indices must have no gap.
#[test]
fn indices_run_on_from_one_allocation_to_the_next() {
	let (_dir, ledger) = ledger_with_purse(test_ledger_deps);
	let first = allocate(&ledger, 3).expect("first allocation");
	let second = allocate(&ledger, 2).expect("second allocation");
	assert_eq!([first, second].concat(), expected_keys(0..5));
}

/// Two allocations that race must not hand out one key twice.
#[test]
fn concurrent_allocations_never_share_an_index() {
	let (_dir, ledger) = ledger_with_purse(test_ledger_deps);
	let count = NonZeroU32::new(50).expect("count is positive");
	let both = futures::future::join(
		ledger.allocate_items(PurseId(0), count),
		ledger.allocate_items(PurseId(0), count),
	);
	let (left, right) = block_on(both);
	let allocated: HashSet<ItemKey> =
		[left.expect("left allocation"), right.expect("right allocation")]
			.concat()
			.into_iter()
			.collect();
	assert_eq!(allocated, expected_keys(0..100).into_iter().collect());
}

/// Each installation has its own indices, so two devices of one identity
/// never derive the same key.
#[test]
fn another_installation_starts_its_own_indices() {
	let (_dir, ledger) = ledger_with_purse(test_ledger_deps);
	allocate(&ledger, 2).expect("first installation allocates");
	let other = InstallationId([0x2e; 32]);
	let other_ledger = NftLedger::new(NftLedgerDeps {
		installation: Arc::new(FixedInstallation(other)),
		..test_ledger_deps(ledger.db.clone())
	});
	let allocated = allocate(&other_ledger, 1).expect("second installation allocates");
	let slot = ItemSlot { purse: PurseId(0), installation: other, index: ItemIndex(0) };
	let expected = item_keys_from_entropy(&TEST_ENTROPY, &[slot]).expect("derives");
	assert_eq!(allocated, expected);
}

/// Items of a purse that does not exist would never be listed or tracked
/// under an owner, so the product learns of its mistake instead.
#[test]
fn allocating_in_an_unknown_purse_is_refused() {
	let (_dir, ledger) = ledger_with_purse(test_ledger_deps);
	let count = NonZeroU32::new(1).expect("count is positive");
	let refused = block_on(ledger.allocate_items(PurseId(7), count));
	assert!(matches!(refused, Err(AllocateItemsError::UnknownPurse(UnknownPurse(PurseId(7))))));
}

/// A failed derivation, as without a session, must not use up indices.
#[test]
fn a_failed_derivation_writes_nothing() {
	let (_dir, ledger) =
		ledger_with_purse(|db| NftLedgerDeps { keys: Arc::new(SignedOut), ..test_ledger_deps(db) });
	let refused = allocate(&ledger, 2);
	assert!(matches!(refused, Err(AllocateItemsError::Derive(AuthorityError::Disconnected))));
	let signed_in = NftLedger::new(test_ledger_deps(ledger.db.clone()));
	assert_eq!(allocate(&signed_in, 1).expect("allocates"), expected_keys(0..1));
}

/// Without an installation there is no path to derive under, and guessing
/// one would put keys where recovery never looks.
#[test]
fn an_unavailable_installation_is_reported() {
	let (_dir, ledger) = ledger_with_purse(|db| NftLedgerDeps {
		installation: Arc::new(NoInstallation),
		..test_ledger_deps(db)
	});
	let refused = allocate(&ledger, 1);
	assert!(matches!(refused, Err(AllocateItemsError::Installation(_))));
}

/// A deriver that answers fewer keys than slots must not leave slots
/// recorded without a key, or with another slot's key.
#[test]
fn a_short_derivation_writes_nothing() {
	let (_dir, ledger) = ledger_with_purse(|db| NftLedgerDeps {
		keys: Arc::new(OneKeyShort),
		..test_ledger_deps(db)
	});
	let refused = allocate(&ledger, 2);
	assert!(matches!(refused, Err(AllocateItemsError::DerivedKeyCount { slots: 2, derived: 1 })));
	let whole = NftLedger::new(test_ledger_deps(ledger.db.clone()));
	assert_eq!(allocate(&whole, 1).expect("allocates"), expected_keys(0..1));
}

/// Indices are `u32`, so a purse that has used nearly all of them refuses
/// rather than wrapping around onto keys it already handed out.
#[test]
fn allocating_past_the_last_index_is_refused() {
	let (_dir, ledger) = ledger_with_purse(test_ledger_deps);
	let last_key = ItemKey([9; 32]);
	let last = u32::MAX - 1;
	insert_item_row(&ledger.db, last_key, last);
	let refused = allocate(&ledger, 2);
	assert!(matches!(
		refused,
		Err(AllocateItemsError::IndicesExhausted { purse: PurseId(0), next: u32::MAX, count: 2 })
	));
}

/// Records that each of `keys` holds an NFT whose instance is its position.
fn hold(ledger: &NftLedger, keys: &[ItemKey]) {
	let held = |(instance, key): (usize, &ItemKey)| {
		let instance = u64::try_from(instance).expect("a test position fits u64");
		(*key, Some(HeldItem { collection: 1, item: 1, instance }))
	};
	let holdings = keys.iter().enumerate().map(held).collect();
	block_on(ledger.record_holdings(holdings)).expect("holdings are recorded");
}

fn page(ledger: &NftLedger, index: u32, size: u32) -> ItemPage {
	let size = NonZeroU32::new(size).expect("size is positive");
	let request = PageRequest { index, size };
	block_on(ledger.available_items(PurseId(0), request)).expect("page reads")
}

/// Products page through a purse one call at a time, so pages must not
/// overlap or skip, and the last one says nothing follows.
#[test]
fn available_items_page_in_slot_order() {
	let (_dir, ledger) = ledger_with_purse(test_ledger_deps);
	let keys = allocate(&ledger, 5).expect("allocates");
	hold(&ledger, &keys);

	let pages =
		[page(&ledger, 0, 2), page(&ledger, 1, 2), page(&ledger, 2, 2), page(&ledger, 3, 2)];

	let expected = [
		ItemPage { items: keys[0..2].to_vec(), has_next: true },
		ItemPage { items: keys[2..4].to_vec(), has_next: true },
		ItemPage { items: keys[4..5].to_vec(), has_next: false },
		ItemPage { items: vec![], has_next: false },
	];
	assert_eq!(pages, expected);
}

/// Only a key that holds an NFT is spendable; one that never received or
/// already gave one away is left out.
#[test]
fn keys_holding_nothing_are_not_available() {
	let (_dir, ledger) = ledger_with_purse(test_ledger_deps);
	let keys = allocate(&ledger, 3).expect("allocates");
	hold(&ledger, &keys[1..2]);

	assert_eq!(page(&ledger, 0, 10), ItemPage { items: vec![keys[1]], has_next: false });
}

/// A deriver with no session.
struct SignedOut;

impl ItemKeyDeriver for SignedOut {
	fn derive_item_keys(&self, _slots: &[ItemSlot]) -> Result<Vec<ItemKey>, AuthorityError> {
		Err(AuthorityError::Disconnected)
	}
}

/// A deriver that drops the last key.
struct OneKeyShort;

impl ItemKeyDeriver for OneKeyShort {
	fn derive_item_keys(&self, slots: &[ItemSlot]) -> Result<Vec<ItemKey>, AuthorityError> {
		let mut keys = item_keys_from_entropy(&TEST_ENTROPY, slots).expect("test entropy derives");
		keys.pop();
		Ok(keys)
	}
}

/// A host that cannot name its installation.
struct NoInstallation;

#[async_trait]
impl InstallationIdProvider for NoInstallation {
	async fn current_installation(&self) -> Result<InstallationId, GenericError> {
		Err(GenericError { reason: "no installation".to_owned() })
	}
}
