use truapi::{
	api::NftPurse,
	versioned::nft_purse::{
		HostNftPurseCreateItemsError, HostNftPurseCreateItemsRequest,
		HostNftPurseCreateItemsResponse, HostNftPurseCreatePurseError,
		HostNftPurseCreatePurseRequest, HostNftPurseCreatePurseResponse,
		HostNftPurseListPursesError, HostNftPurseListPursesRequest, HostNftPurseListPursesResponse,
		HostNftPurseListenForPaymentRequest, HostNftPurseQueryPurseError,
		HostNftPurseQueryPurseRequest, HostNftPurseQueryPurseResponse,
	},
};

use super::*;
use crate::{
	encrypted_storage::{EncryptedStorage, MemoryEncryptedStorage},
	nft::{PurseId, test_ledger},
	nft_purse::{FixedStaticSecret, TEST_STATIC_SECRET},
	store::open_temporary_core_db,
};

const PRODUCT: &str = "tickets.dot";

/// The id of the first purse a test creates, its "Tickets" purse.
const TICKETS: u32 = 0;

/// A product host over a fresh core database, with the directory that keeps
/// the database alive.
fn nft_host(
	platform: Arc<StubPlatform>,
	with_session: bool,
) -> (tempfile::TempDir, ProductRuntimeHost) {
	let encrypted = Arc::new(MemoryEncryptedStorage::default());
	nft_host_with_storage(platform, with_session, encrypted)
}

/// A product host over a fresh core database and `encrypted`, with the
/// directory that keeps the database alive.
pub fn nft_host_with_storage(
	platform: Arc<StubPlatform>,
	with_session: bool,
	encrypted: Arc<dyn EncryptedStorage>,
) -> (tempfile::TempDir, ProductRuntimeHost) {
	let services = payment_services_over(platform);
	let (dir, db) = open_temporary_core_db();
	assert!(services.install_core_db(db.clone()));
	assert!(services.install_nft_ledger(test_ledger(db)));
	assert!(services.install_encrypted_storage(encrypted));
	assert!(services.install_static_receive_secret(test_static_secret()));
	(dir, payment_host(&services, PRODUCT, with_session))
}

/// The static receive secret every test host opens `Static` cheques with.
fn test_static_secret() -> Arc<FixedStaticSecret> {
	Arc::new(FixedStaticSecret(TEST_STATIC_SECRET))
}

fn create(
	host: &ProductRuntimeHost,
	name: &str,
) -> Result<HostNftPurseCreatePurseResponse, CallError<HostNftPurseCreatePurseError>> {
	let request = v01::HostNftPurseCreatePurseRequest { name: name.to_owned() };
	futures::executor::block_on(
		host.create_purse(&CallContext::default(), HostNftPurseCreatePurseRequest::V1(request)),
	)
}

fn list(
	host: &ProductRuntimeHost,
	owner: v01::NftPurseOwner,
) -> Result<HostNftPurseListPursesResponse, CallError<HostNftPurseListPursesError>> {
	let request = v01::HostNftPurseListPursesRequest { owner };
	futures::executor::block_on(
		host.list_purses(&CallContext::default(), HostNftPurseListPursesRequest::V1(request)),
	)
}

fn create_items(
	host: &ProductRuntimeHost,
	purse: u32,
	count: u32,
) -> Result<HostNftPurseCreateItemsResponse, CallError<HostNftPurseCreateItemsError>> {
	let request = v01::HostNftPurseCreateItemsRequest { purse: v01::NftPurseId(purse), count };
	futures::executor::block_on(
		host.create_items(&CallContext::default(), HostNftPurseCreateItemsRequest::V1(request)),
	)
}

fn item_count(created: HostNftPurseCreateItemsResponse) -> usize {
	let HostNftPurseCreateItemsResponse::V1(response) = created;
	response.items.len()
}

fn create_items_refusal(
	error: v01::NftPurseCreateItemsError,
) -> CallError<HostNftPurseCreateItemsError> {
	CallError::Domain(HostNftPurseCreateItemsError::V1(error))
}

fn query(
	host: &ProductRuntimeHost,
	purse: u32,
	page_size: u32,
) -> Result<HostNftPurseQueryPurseResponse, CallError<HostNftPurseQueryPurseError>> {
	let request =
		v01::HostNftPurseQueryPurseRequest { purse: v01::NftPurseId(purse), page: 0, page_size };
	futures::executor::block_on(
		host.query_purse(&CallContext::default(), HostNftPurseQueryPurseRequest::V1(request)),
	)
}

fn query_refusal(error: v01::NftPurseQueryPurseError) -> CallError<HostNftPurseQueryPurseError> {
	CallError::Domain(HostNftPurseQueryPurseError::V1(error))
}

fn empty_page() -> HostNftPurseQueryPurseResponse {
	let page = v01::NftPurseInfoPage { has_next: false, items: vec![] };
	HostNftPurseQueryPurseResponse::V1(v01::HostNftPurseQueryPurseResponse { page })
}

fn listed(purses: Vec<v01::NftPursePurse>) -> HostNftPurseListPursesResponse {
	HostNftPurseListPursesResponse::V1(v01::HostNftPurseListPursesResponse { purses })
}

fn purse(id: u32, name: &str) -> v01::NftPursePurse {
	v01::NftPursePurse { id: v01::NftPurseId(id), name: name.to_owned() }
}

fn requested_permissions(platform: &StubPlatform) -> Vec<v01::RemotePermissionRequest> {
	platform
		.remote_permission_requests
		.lock()
		.expect("requests mutex poisoned")
		.clone()
}

// A product's own purses round-trip through the trait without any prompt.
#[test]
fn a_product_lists_the_purse_it_created_without_a_prompt() {
	let platform = Arc::new(StubPlatform::default());
	let (_dir, host) = nft_host(platform.clone(), true);

	let created = create(&host, "Tickets");
	let own = list(&host, v01::NftPurseOwner::Product(PRODUCT.to_owned()));

	let response = v01::HostNftPurseCreatePurseResponse { purse: v01::NftPurseId(0) };
	assert_eq!(created, Ok(HostNftPurseCreatePurseResponse::V1(response)));
	assert_eq!(own, Ok(listed(vec![purse(0, "Tickets")])));
	assert_eq!(requested_permissions(&platform), vec![]);
}

// Root's purses hold the user's main purse, so a product sees them only with
// the user's grant for that owner.
#[test]
fn listing_roots_purses_without_the_users_grant_is_denied() {
	let platform = Arc::new(StubPlatform { remote_permission_denied: true, ..Default::default() });
	let (_dir, host) = nft_host(platform.clone(), true);

	let refused = list(&host, v01::NftPurseOwner::Root);

	let denied = HostNftPurseListPursesError::V1(v01::NftPurseListPursesError::Denied);
	assert_eq!(refused, Err(CallError::Domain(denied)));
	let read_root = v01::RemotePermission::NftPurseRead { owner: v01::NftPurseOwner::Root };
	assert_eq!(
		requested_permissions(&platform),
		vec![v01::RemotePermissionRequest { permission: read_root }]
	);
}

#[test]
fn listing_roots_purses_with_the_users_grant_returns_the_main_purse() {
	let (_dir, host) = nft_host(Arc::new(StubPlatform::default()), true);

	assert_eq!(
		list(&host, v01::NftPurseOwner::Root),
		Ok(listed(vec![purse(PurseId::MAIN.0, "Main")]))
	);
}

// The RFC requires an empty list, not an error, for a product without purses.
#[test]
fn another_product_without_purses_lists_none() {
	let (_dir, host) = nft_host(Arc::new(StubPlatform::default()), true);

	assert_eq!(
		list(&host, v01::NftPurseOwner::Product("other.dot".to_owned())),
		Ok(listed(vec![]))
	);
}

// A name over the limit is the product's mistake, so it is a domain error and
// not a host failure.
#[test]
fn a_name_over_64_bytes_is_a_domain_error() {
	let (_dir, host) = nft_host(Arc::new(StubPlatform::default()), true);

	assert_eq!(
		create(&host, &"x".repeat(65)),
		Err(CallError::Domain(HostNftPurseCreatePurseError::V1(v01::GenericError {
			reason: "purse name is 65 bytes, expected at most 64".to_owned(),
		})))
	);
}

// Purses belong to the signed-in user, so neither call runs without a paired
// session.
#[test]
fn purses_need_a_paired_session() {
	let (_dir, host) = nft_host(Arc::new(StubPlatform::default()), false);

	assert_eq!(create(&host, "Tickets"), Err(CallError::Denied));
	assert_eq!(
		list(&host, v01::NftPurseOwner::Product(PRODUCT.to_owned())),
		Err(CallError::Denied)
	);
	assert_eq!(create_items(&host, PurseId::MAIN.0, 1), Err(CallError::Denied));
	assert_eq!(query(&host, PurseId::MAIN.0, 1), Err(CallError::Denied));
}

// The empty product id shares Root's empty tag, so naming it must fail before
// any prompt rather than show Root's purses under a blank owner. Like any
// product id a product sends that does not parse, it is a malformed request.
#[test]
fn an_empty_product_owner_is_refused_without_a_prompt() {
	let platform = Arc::new(StubPlatform::default());
	let (_dir, host) = nft_host(platform.clone(), true);

	let refused = list(&host, v01::NftPurseOwner::Product(String::new()));

	let reason = "product_id must not be empty".to_owned();
	assert_eq!(refused, Err(CallError::MalformedFrame { reason }));
	assert_eq!(requested_permissions(&platform), vec![]);
}

// A host that configured no core database has nowhere to keep purses.
#[test]
fn a_host_without_a_core_database_does_not_serve_purses() {
	let host = payment_host(&payment_services(), PRODUCT, true);

	assert_eq!(create(&host, "Tickets"), Err(CallError::Unsupported));
}

// The RFC bounds one call to 1..=1000 keys.
#[test]
fn create_items_takes_between_1_and_1000_keys() {
	let (_dir, host) = nft_host(Arc::new(StubPlatform::default()), true);
	create(&host, "Tickets").expect("purse is created");

	let counts =
		[0, 1, 1000, 1001].map(|count| create_items(&host, TICKETS, count).map(item_count));

	let out_of_range = create_items_refusal(v01::NftPurseCreateItemsError::CountOutOfRange);
	assert_eq!(counts, [Err(out_of_range.clone()), Ok(1), Ok(1000), Err(out_of_range)]);
}

// An out-of-range count is the product's mistake, so it is refused before
// the user is asked for Root's purse, which a valid count would prompt for.
#[test]
fn an_out_of_range_count_is_refused_without_a_prompt() {
	let platform = Arc::new(StubPlatform::default());
	let (_dir, host) = nft_host(platform.clone(), true);

	let _ = create_items(&host, PurseId::MAIN.0, 1001);

	assert_eq!(requested_permissions(&platform), vec![]);
}

// Keys in Root's purses receive the user's own NFTs, so creating them needs
// the same grant as reading those purses.
#[test]
fn creating_items_in_roots_purse_without_the_users_grant_is_denied() {
	let platform = Arc::new(StubPlatform { remote_permission_denied: true, ..Default::default() });
	let (_dir, host) = nft_host(platform.clone(), true);

	let refused = create_items(&host, PurseId::MAIN.0, 1);

	assert_eq!(refused, Err(create_items_refusal(v01::NftPurseCreateItemsError::Denied)));
	let read_root = v01::RemotePermission::NftPurseRead { owner: v01::NftPurseOwner::Root };
	assert_eq!(
		requested_permissions(&platform),
		vec![v01::RemotePermissionRequest { permission: read_root }]
	);
}

#[test]
fn creating_items_in_roots_purse_with_the_users_grant_returns_the_keys() {
	let (_dir, host) = nft_host(Arc::new(StubPlatform::default()), true);

	let created = create_items(&host, PurseId::MAIN.0, 3).map(item_count);

	assert_eq!(created, Ok(3));
}

// The user is never asked about a purse that does not exist.
#[test]
fn creating_items_in_an_unknown_purse_is_refused_without_a_prompt() {
	let platform = Arc::new(StubPlatform::default());
	let (_dir, host) = nft_host(platform.clone(), true);

	let refused = create_items(&host, 7, 1);

	let unknown = v01::GenericError { reason: "purse 7 does not exist".to_owned() };
	let expected = create_items_refusal(v01::NftPurseCreateItemsError::Other(unknown));
	assert_eq!(refused, Err(expected));
	assert_eq!(requested_permissions(&platform), vec![]);
}

// The RFC bounds a page to 1..=100 items.
#[test]
fn query_purse_takes_pages_of_1_to_100_items() {
	let (_dir, host) = nft_host(Arc::new(StubPlatform::default()), true);
	create(&host, "Tickets").expect("purse is created");

	let pages = [0, 1, 100, 101].map(|size| query(&host, TICKETS, size));

	let out_of_bounds = query_refusal(v01::NftPurseQueryPurseError::PageSizeOutOfBounds);
	assert_eq!(
		pages,
		[Err(out_of_bounds.clone()), Ok(empty_page()), Ok(empty_page()), Err(out_of_bounds)]
	);
}

// An out-of-range page size is the product's mistake, so it is refused
// before the user is asked for Root's purse, which a valid size would prompt
// for.
#[test]
fn an_out_of_range_page_size_is_refused_without_a_prompt() {
	let platform = Arc::new(StubPlatform::default());
	let (_dir, host) = nft_host(platform.clone(), true);

	let _ = query(&host, PurseId::MAIN.0, 101);

	assert_eq!(requested_permissions(&platform), vec![]);
}

// Keys that were created but hold nothing on chain are not spendable, so the
// RFC leaves them out.
#[test]
fn created_keys_that_hold_nothing_are_not_listed() {
	let (_dir, host) = nft_host(Arc::new(StubPlatform::default()), true);
	create(&host, "Tickets").expect("purse is created");
	create_items(&host, TICKETS, 3).expect("keys are created");

	assert_eq!(query(&host, TICKETS, 10), Ok(empty_page()));
}

#[test]
fn querying_roots_purse_without_the_users_grant_is_denied() {
	let platform = Arc::new(StubPlatform { remote_permission_denied: true, ..Default::default() });
	let (_dir, host) = nft_host(platform.clone(), true);

	let refused = query(&host, PurseId::MAIN.0, 10);

	assert_eq!(refused, Err(query_refusal(v01::NftPurseQueryPurseError::Denied)));
	let read_root = v01::RemotePermission::NftPurseRead { owner: v01::NftPurseOwner::Root };
	assert_eq!(
		requested_permissions(&platform),
		vec![v01::RemotePermissionRequest { permission: read_root }]
	);
}

#[test]
fn querying_an_unknown_purse_is_refused_without_a_prompt() {
	let platform = Arc::new(StubPlatform::default());
	let (_dir, host) = nft_host(platform.clone(), true);

	let refused = query(&host, 7, 10);

	let unknown = v01::GenericError { reason: "purse 7 does not exist".to_owned() };
	let expected = query_refusal(v01::NftPurseQueryPurseError::Other(unknown));
	assert_eq!(refused, Err(expected));
	assert_eq!(requested_permissions(&platform), vec![]);
}

// Methods of later stages stay unavailable, so products can feature-detect them.
#[test]
fn methods_of_later_stages_are_unavailable() {
	let (_dir, host) = nft_host(Arc::new(StubPlatform::default()), true);
	let request =
		v01::HostNftPurseListenForPaymentRequest { key: v01::NftPurseReceiveKey([1; 32]) };

	let mut payments = futures::executor::block_on(host.listen_for_payment(
		&CallContext::default(),
		HostNftPurseListenForPaymentRequest::V1(request),
	));

	let first = futures::executor::block_on(futures::StreamExt::next(&mut payments));
	assert_eq!(first, Some(Err(CallError::unavailable())));
}

mod cheques;
mod deposits;

mod operations {
	//! The chain operations of NFT purses through the product-facing trait.

	use core::num::NonZeroU32;

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
	pub struct Purses {
		_dir: tempfile::TempDir,
		pub services: Arc<RuntimeServices>,
		pub ledger: NftLedger,
		pub host: ProductRuntimeHost,
	}

	/// [`Purses`] over in-memory encrypted storage.
	pub fn product_purses(platform: Arc<StubPlatform>) -> Purses {
		product_purses_over(platform, Arc::new(MemoryEncryptedStorage::default()))
	}

	/// [`product_purses`] with receive key secrets kept in `encrypted`.
	pub fn product_purses_over(
		platform: Arc<StubPlatform>,
		encrypted: Arc<dyn EncryptedStorage>,
	) -> Purses {
		let services = payment_services_over(platform);
		let (dir, db) = open_temporary_core_db();
		let ledger = test_ledger(db.clone());
		assert!(services.install_core_db(db));
		assert!(services.install_nft_ledger(ledger.clone()));
		assert!(services.install_encrypted_storage(encrypted));
		assert!(services.install_static_receive_secret(test_static_secret()));
		let host = payment_host(&services, PRODUCT, true);
		create(&host, "Tickets").unwrap();
		create(&host, "Spare").unwrap();
		Purses { _dir: dir, services, ledger, host }
	}

	impl Purses {
		/// `count` keys of `purse`, each holding an NFT.
		pub fn held_items(&self, purse: PurseId, count: u32) -> Vec<v01::NftPurseItemKey> {
			let count = NonZeroU32::new(count).unwrap();
			let keys = block_on(self.ledger.allocate_items(purse, count)).unwrap();
			let held = HeldItem { collection: 1, item: 1, instance: 7 };
			let holdings = keys.iter().map(|key| (*key, Some(held))).collect();
			block_on(self.ledger.record_holdings(holdings)).unwrap();
			keys.into_iter().map(|key| v01::NftPurseItemKey(key.0)).collect()
		}

		/// A purse of Root's, which no product owns.
		pub fn root_purse(&self) -> PurseId {
			block_on(self.ledger.create_purse("Recovered".to_owned(), PurseTag::default())).unwrap()
		}

		/// A host for [`OTHER_PRODUCT`] over the same services.
		pub fn other_product(&self) -> ProductRuntimeHost {
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
		let mut statuses =
			block_on(host.observe_operation_status(&CallContext::default(), request));
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
		let in_progress =
			HostNftPurseObserveOperationStatusItem::V1(v01::NftPurseStatus::InProgress);
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

		assert_eq!(
			refused,
			Err(rebalance_refusal(v01::NftPurseRebalancePurseError::PurseNotFound))
		);
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
	fn draining_a_root_purse_the_user_confirms_registers_the_transfers() {
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

		let not_found =
			HostNftPurseTransferItemError::V1(v01::NftPurseTransferItemError::ItemNotFound);
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
	fn an_item_spent_by_a_registered_transfer_is_not_listed() {
		let purses = product_purses(Arc::new(StubPlatform::default()));
		let items = purses.held_items(PurseId(0), 1);

		rebalance(&purses.host, 0, 1, items).unwrap();

		assert_eq!(query(&purses.host, 0, 10), Ok(empty_page()));
	}
}
