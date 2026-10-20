use truapi::{
	api::NftPurse,
	versioned::nft_purse::{
		HostNftPurseCreateItemsRequest, HostNftPurseCreatePurseError,
		HostNftPurseCreatePurseRequest, HostNftPurseCreatePurseResponse,
		HostNftPurseListPursesError, HostNftPurseListPursesRequest, HostNftPurseListPursesResponse,
	},
};

use super::*;
use crate::{
	nft::NftLedger,
	nft_purse::NftPurses,
	store::{Db, DbConfig, DbLocation, core_migrations},
};

const PRODUCT: &str = "tickets.dot";

fn nft_services(platform: Arc<StubPlatform>) -> Arc<RuntimeServices> {
	let services = payment_services_over(platform);
	let config = DbConfig { location: DbLocation::Memory, migrations: core_migrations, readers: 1 };
	let db = futures::executor::block_on(Db::open(config)).expect("in-memory core database opens");
	assert!(services.install_nft_purses(NftPurses::new(NftLedger::new(db))));
	services
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

fn listed(purses: Vec<v01::NftPursePurse>) -> HostNftPurseListPursesResponse {
	HostNftPurseListPursesResponse::V1(v01::HostNftPurseListPursesResponse { purses })
}

fn purse(id: u32, name: &str) -> v01::NftPursePurse {
	v01::NftPursePurse { id, name: name.to_owned() }
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
	let host = payment_host(&nft_services(platform.clone()), PRODUCT, true);

	let created = create(&host, "Tickets");
	let own = list(&host, v01::NftPurseOwner::Product(PRODUCT.to_owned()));

	assert_eq!(
		(created, own, requested_permissions(&platform)),
		(
			Ok(HostNftPurseCreatePurseResponse::V1(v01::HostNftPurseCreatePurseResponse {
				purse: 0
			})),
			Ok(listed(vec![purse(0, "Tickets")])),
			vec![],
		)
	);
}

// Root's purses hold the user's main purse, so a product sees them only with
// the user's grant for that owner.
#[test]
fn listing_roots_purses_without_the_users_grant_is_denied() {
	let platform = Arc::new(StubPlatform { remote_permission_denied: true, ..Default::default() });
	let host = payment_host(&nft_services(platform.clone()), PRODUCT, true);

	let refused = list(&host, v01::NftPurseOwner::Root);

	assert_eq!(
		(refused, requested_permissions(&platform)),
		(
			Err(CallError::Domain(HostNftPurseListPursesError::V1(
				v01::NftPurseListPursesError::Denied
			))),
			vec![v01::RemotePermissionRequest {
				permission: v01::RemotePermission::NftPurseRead { owner: v01::NftPurseOwner::Root },
			}],
		)
	);
}

#[test]
fn listing_roots_purses_with_the_users_grant_returns_the_main_purse() {
	let host = payment_host(&nft_services(Arc::new(StubPlatform::default())), PRODUCT, true);

	assert_eq!(list(&host, v01::NftPurseOwner::Root), Ok(listed(vec![purse(u32::MAX, "Main")])));
}

// The RFC requires an empty list, not an error, for a product without purses.
#[test]
fn another_product_without_purses_lists_none() {
	let host = payment_host(&nft_services(Arc::new(StubPlatform::default())), PRODUCT, true);

	assert_eq!(
		list(&host, v01::NftPurseOwner::Product("other.dot".to_owned())),
		Ok(listed(vec![]))
	);
}

// A name over the limit is the product's mistake, so it is a domain error and
// not a host failure.
#[test]
fn a_name_over_64_bytes_is_a_domain_error() {
	let host = payment_host(&nft_services(Arc::new(StubPlatform::default())), PRODUCT, true);

	assert_eq!(
		create(&host, &"x".repeat(65)),
		Err(CallError::Domain(HostNftPurseCreatePurseError::V1(v01::GenericError {
			reason: "purse name is 65 bytes, expected at most 64".to_owned(),
		})))
	);
}

#[test]
fn purses_need_a_paired_session() {
	let host = payment_host(&nft_services(Arc::new(StubPlatform::default())), PRODUCT, false);

	assert_eq!(create(&host, "Tickets"), Err(CallError::Denied));
}

// A host that configured no core database has nowhere to keep purses.
#[test]
fn a_host_without_a_core_database_does_not_serve_purses() {
	let host = payment_host(&payment_services(), PRODUCT, true);

	assert_eq!(create(&host, "Tickets"), Err(CallError::Unsupported));
}

// Methods of later stages stay unavailable, so products can feature-detect them.
#[test]
fn methods_of_later_stages_are_unavailable() {
	let host = payment_host(&nft_services(Arc::new(StubPlatform::default())), PRODUCT, true);
	let request = v01::HostNftPurseCreateItemsRequest { purse: 0, count: 1 };

	let created = futures::executor::block_on(
		host.create_items(&CallContext::default(), HostNftPurseCreateItemsRequest::V1(request)),
	);

	assert_eq!(created, Err(CallError::unavailable()));
}
