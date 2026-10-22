//! The chain operations of NFT purses through the product-facing trait.

use std::num::NonZeroU32;

use futures::{StreamExt, executor::block_on};
use truapi::versioned::nft_purse::{
	HostNftPurseObserveOperationStatusError, HostNftPurseObserveOperationStatusItem,
	HostNftPurseObserveOperationStatusRequest, HostNftPurseRebalancePurseError,
	HostNftPurseRebalancePurseRequest, HostNftPurseRebalancePurseResponse,
	HostNftPurseTransferItemError, HostNftPurseTransferItemRequest,
	HostNftPurseTransferItemResponse,
};

use super::*;
use crate::{
	nft::{HeldItem, NftLedger, PurseTag},
	platform::{NftPurseDrainDestination, NftPurseDrainReview},
};

const OTHER_PRODUCT: &str = "other.dot";
const KEY: [u8; 32] = [0x11; 32];

/// A product host over a fresh core database, and the ledger it serves,
/// with purses 0 and 1 owned by [`PRODUCT`].
struct Purses {
	_dir: tempfile::TempDir,
	services: Arc<RuntimeServices>,
	ledger: NftLedger,
	host: ProductRuntimeHost,
}

fn product_purses(platform: Arc<StubPlatform>) -> Purses {
	let services = payment_services_over(platform);
	let (dir, db) = open_temporary_core_db();
	let ledger = test_ledger(db.clone());
	assert!(services.install_core_db(db));
	assert!(services.install_nft_ledger(ledger.clone()));
	let host = payment_host(&services, PRODUCT, true);
	create(&host, "Tickets").unwrap();
	create(&host, "Spare").unwrap();
	Purses { _dir: dir, services, ledger, host }
}

impl Purses {
	/// `count` keys of `purse`, each holding an NFT.
	fn held_items(&self, purse: PurseId, count: u32) -> Vec<v01::NftPurseItemKey> {
		let count = NonZeroU32::new(count).unwrap();
		let keys = block_on(self.ledger.allocate_items(purse, count)).unwrap();
		let held = HeldItem { collection: 1, item: 1, instance: 7 };
		let holdings = keys.iter().map(|key| (*key, Some(held))).collect();
		block_on(self.ledger.record_holdings(holdings)).unwrap();
		keys.into_iter().map(|key| v01::NftPurseItemKey(key.0)).collect()
	}

	/// A purse of Root's, which no product owns.
	fn root_purse(&self) -> PurseId {
		block_on(self.ledger.create_purse("Recovered".to_owned(), PurseTag::default())).unwrap()
	}

	fn other_product(&self) -> ProductRuntimeHost {
		payment_host(&self.services, OTHER_PRODUCT, true)
	}
}

fn rebalance(
	host: &ProductRuntimeHost,
	from: u32,
	to: u32,
	keys: Vec<v01::NftPurseItemKey>,
) -> Result<HostNftPurseRebalancePurseResponse, CallError<HostNftPurseRebalancePurseError>> {
	let idempotency_key = v01::NftPurseIdempotencyKey(KEY);
	let (from, to) = (v01::NftPurseId(from), v01::NftPurseId(to));
	let request = v01::HostNftPurseRebalancePurseRequest { from, to, keys, idempotency_key };
	let request = HostNftPurseRebalancePurseRequest::V1(request);
	block_on(host.rebalance_purse(&CallContext::default(), request))
}

fn rebalance_refusal(
	error: v01::NftPurseRebalancePurseError,
) -> CallError<HostNftPurseRebalancePurseError> {
	CallError::Domain(HostNftPurseRebalancePurseError::V1(error))
}

fn transfer(
	host: &ProductRuntimeHost,
	item: v01::NftPurseItemKey,
) -> Result<HostNftPurseTransferItemResponse, CallError<HostNftPurseTransferItemError>> {
	let destination = v01::NftPurseItemKey([0xee; 32]);
	let idempotency_key = v01::NftPurseIdempotencyKey(KEY);
	let request = v01::HostNftPurseTransferItemRequest { item, destination, idempotency_key };
	let request = HostNftPurseTransferItemRequest::V1(request);
	block_on(host.transfer_item(&CallContext::default(), request))
}

fn first_status(
	host: &ProductRuntimeHost,
) -> Result<
	HostNftPurseObserveOperationStatusItem,
	CallError<HostNftPurseObserveOperationStatusError>,
> {
	let idempotency_key = v01::NftPurseIdempotencyKey(KEY);
	let request = v01::HostNftPurseObserveOperationStatusRequest { idempotency_key };
	let request = HostNftPurseObserveOperationStatusRequest::V1(request);
	let mut statuses = block_on(host.observe_operation_status(&CallContext::default(), request));
	block_on(statuses.next()).expect("a status or an interruption")
}

fn drain_reviews(platform: &StubPlatform) -> Vec<NftPurseDrainReview> {
	platform.nft_purse_drain_reviews.lock().unwrap().clone()
}

#[test]
fn a_rebalance_returns_once_registered_and_its_status_follows_by_key() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let items = purses.held_items(PurseId(0), 2);

	let registered = rebalance(&purses.host, 0, 1, items);

	assert_eq!(registered, Ok(HostNftPurseRebalancePurseResponse::V1));
	let in_progress = HostNftPurseObserveOperationStatusItem::V1(v01::NftPurseStatus::InProgress);
	assert_eq!(first_status(&purses.host), Ok(in_progress));
}

#[test]
fn a_rebalance_of_no_keys_answers_no_items() {
	let purses = product_purses(Arc::new(StubPlatform::default()));

	let refused = rebalance(&purses.host, 0, 1, vec![]);

	assert_eq!(refused, Err(rebalance_refusal(v01::NftPurseRebalancePurseError::NoItems)));
}

#[test]
fn a_rebalance_into_a_purse_that_does_not_exist_answers_purse_not_found() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let items = purses.held_items(PurseId(0), 1);

	let refused = rebalance(&purses.host, 0, 9, items);

	assert_eq!(refused, Err(rebalance_refusal(v01::NftPurseRebalancePurseError::PurseNotFound)));
}

#[test]
fn a_second_rebalance_under_the_same_key_answers_operation_exists() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let items = purses.held_items(PurseId(0), 2);
	rebalance(&purses.host, 0, 1, items[..1].to_vec()).unwrap();

	let again = rebalance(&purses.host, 0, 1, items[1..].to_vec());

	let exists = rebalance_refusal(v01::NftPurseRebalancePurseError::OperationExists);
	assert_eq!(again, Err(exists));
}

#[test]
fn a_key_not_in_the_source_purse_answers_item_not_found() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let elsewhere = purses.held_items(PurseId(1), 1);

	let refused = rebalance(&purses.host, 0, 1, elsewhere);

	assert_eq!(refused, Err(rebalance_refusal(v01::NftPurseRebalancePurseError::ItemNotFound)));
}

// Draining Root's purse transfers the user's own items, so the user is asked on
// every call and a refusal leaves everything in place.
#[test]
fn draining_a_root_purse_asks_the_user_and_a_refusal_is_denied() {
	let platform = Arc::new(StubPlatform::default());
	let purses = product_purses(platform.clone());
	let root = purses.root_purse();
	let items = purses.held_items(root, 2);

	let refused = rebalance(&purses.host, root.0, 0, items);

	assert_eq!(refused, Err(rebalance_refusal(v01::NftPurseRebalancePurseError::Denied)));
	let review = NftPurseDrainReview {
		calling_product_id: PRODUCT.parse().unwrap(),
		from_purse: root.0,
		owner: v01::NftPurseOwner::Root,
		item_count: 2,
		destination: NftPurseDrainDestination::Purse { purse: 0 },
	};
	assert_eq!(drain_reviews(&platform), vec![review]);
}

#[test]
fn draining_a_root_purse_the_user_confirms_registers_the_moves() {
	let platform = StubPlatform { nft_purse_drain_confirmed: true, ..Default::default() };
	let purses = product_purses(Arc::new(platform));
	let root = purses.root_purse();
	let items = purses.held_items(root, 1);

	assert_eq!(
		rebalance(&purses.host, root.0, 0, items),
		Ok(HostNftPurseRebalancePurseResponse::V1)
	);
}

#[test]
fn a_transfer_of_an_item_in_no_purse_answers_item_not_found() {
	let purses = product_purses(Arc::new(StubPlatform::default()));

	let refused = transfer(&purses.host, v01::NftPurseItemKey([0x55; 32]));

	let not_found = HostNftPurseTransferItemError::V1(v01::NftPurseTransferItemError::ItemNotFound);
	assert_eq!(refused, Err(CallError::Domain(not_found)));
}

#[test]
fn a_transfer_of_an_own_item_registers_without_a_prompt() {
	let platform = Arc::new(StubPlatform::default());
	let purses = product_purses(platform.clone());
	let items = purses.held_items(PurseId(0), 1);

	assert_eq!(transfer(&purses.host, items[0]), Ok(HostNftPurseTransferItemResponse::V1));
	assert_eq!(drain_reviews(&platform), vec![]);
}

// A product only follows its own operations: the same key from another
// product names nothing.
#[test]
fn an_operation_is_not_found_by_another_product_or_an_unused_key() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let items = purses.held_items(PurseId(0), 1);
	rebalance(&purses.host, 0, 1, items).unwrap();
	let not_found = CallError::Domain(HostNftPurseObserveOperationStatusError::V1(
		v01::NftPurseObserveOperationStatusError::OperationNotFound,
	));

	assert_eq!(first_status(&purses.other_product()), Err(not_found.clone()));
	let fresh = product_purses(Arc::new(StubPlatform::default()));
	assert_eq!(first_status(&fresh.host), Err(not_found));
}

#[test]
fn an_item_spent_by_a_registered_move_is_not_listed() {
	let purses = product_purses(Arc::new(StubPlatform::default()));
	let items = purses.held_items(PurseId(0), 1);

	rebalance(&purses.host, 0, 1, items).unwrap();

	assert_eq!(query(&purses.host, 0, 10), Ok(empty_page()));
}
