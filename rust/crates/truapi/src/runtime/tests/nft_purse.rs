use truapi::{
	api::NftPurse,
	versioned::nft_purse::{
		HostNftPurseCreateItemsError, HostNftPurseCreateItemsRequest,
		HostNftPurseCreateItemsResponse, HostNftPurseCreatePurseError,
		HostNftPurseCreatePurseRequest, HostNftPurseCreatePurseResponse,
		HostNftPurseCreateReceiveKeyRequest, HostNftPurseListPursesError,
		HostNftPurseListPursesRequest, HostNftPurseListPursesResponse, HostNftPurseQueryPurseError,
		HostNftPurseQueryPurseRequest, HostNftPurseQueryPurseResponse,
	},
};

use super::*;
use crate::{
	nft::{PurseId, test_ledger},
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
	let services = payment_services_over(platform);
	let (dir, db) = open_temporary_core_db();
	assert!(services.install_core_db(db.clone()));
	assert!(services.install_nft_ledger(test_ledger(db)));
	(dir, payment_host(&services, PRODUCT, with_session))
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
	let request = v01::HostNftPurseCreateReceiveKeyRequest { into: v01::NftPurseId(0) };

	let created = futures::executor::block_on(host.create_receive_key(
		&CallContext::default(),
		HostNftPurseCreateReceiveKeyRequest::V1(request),
	));

	assert_eq!(created, Err(CallError::unavailable()));
}

mod operations;
