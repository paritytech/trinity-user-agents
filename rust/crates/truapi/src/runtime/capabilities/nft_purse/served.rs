//! The NFT purse calls a host with a core database serves.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::NftPurse,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		nft_purse::{
			HostNftPurseCreateItemsError, HostNftPurseCreateItemsRequest,
			HostNftPurseCreateItemsResponse, HostNftPurseCreatePurseError,
			HostNftPurseCreatePurseRequest, HostNftPurseCreatePurseResponse,
			HostNftPurseListPursesError, HostNftPurseListPursesRequest,
			HostNftPurseListPursesResponse,
		},
	},
};

use super::super::from_latest_error;
use crate::{
	nft::{AllocateItemsError, CreatePurseError, PurseId},
	nft_purse::{
		self, CreateProductPurseError, ItemCount, NftPurses, OwnerLookupError, PurseOwner,
	},
	runtime::{ProductConnection, ProductRuntimeHost},
	store::DbError,
};

impl ProductConnection {
	/// NFT purses, or `Unsupported` when the host configured no core database.
	fn nft_purses<E>(&self) -> Result<NftPurses, CallError<E>> {
		self.services.nft_purses().ok_or(CallError::Unsupported)
	}
}

#[truapi::async_trait]
impl NftPurse for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "nft_purse.create_purse"))]
	async fn create_purse(
		&self,
		_cx: &CallContext,
		request: HostNftPurseCreatePurseRequest,
	) -> Result<HostNftPurseCreatePurseResponse, CallError<HostNftPurseCreatePurseError>> {
		let version = request.version();
		self.create_nft_purse(request.into_latest())
			.await
			.map(|response| HostNftPurseCreatePurseResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.create_items"))]
	async fn create_items(
		&self,
		_cx: &CallContext,
		request: HostNftPurseCreateItemsRequest,
	) -> Result<HostNftPurseCreateItemsResponse, CallError<HostNftPurseCreateItemsError>> {
		let version = request.version();
		self.create_nft_items(request.into_latest())
			.await
			.map(|response| HostNftPurseCreateItemsResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.list_purses"))]
	async fn list_purses(
		&self,
		_cx: &CallContext,
		request: HostNftPurseListPursesRequest,
	) -> Result<HostNftPurseListPursesResponse, CallError<HostNftPurseListPursesError>> {
		let version = request.version();
		self.list_nft_purses(request.into_latest())
			.await
			.map(|response| HostNftPurseListPursesResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}
}

impl ProductRuntimeHost {
	async fn create_nft_purse(
		&self,
		request: latest::HostNftPurseCreatePurseRequest,
	) -> Result<latest::HostNftPurseCreatePurseResponse, CallError<latest::GenericError>> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let latest::HostNftPurseCreatePurseRequest { name } = request;
		let created = purses.create_purse(&self.connection.product.product_id, name).await;
		let purse = created.map_err(create_purse_failure)?;
		Ok(latest::HostNftPurseCreatePurseResponse { purse: latest::NftPurseId(purse.0) })
	}

	async fn list_nft_purses(
		&self,
		request: latest::HostNftPurseListPursesRequest,
	) -> Result<
		latest::HostNftPurseListPursesResponse,
		CallError<latest::HostNftPurseListPursesError>,
	> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let latest::HostNftPurseListPursesRequest { owner } = request;
		let owner = PurseOwner::parse(&owner)?;
		let caller = &self.connection.product.product_id;
		if let Some(grant) = nft_purse::read_grant(caller, &owner) {
			let denied = latest::HostNftPurseListPursesError::Denied;
			self.connection.require_remote_permission(grant, denied).await?;
		}
		let listed = purses.list_purses(&owner).await.map_err(host_failure)?;
		Ok(latest::HostNftPurseListPursesResponse { purses: listed })
	}

	async fn create_nft_items(
		&self,
		request: latest::HostNftPurseCreateItemsRequest,
	) -> Result<
		latest::HostNftPurseCreateItemsResponse,
		CallError<latest::HostNftPurseCreateItemsError>,
	> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let latest::HostNftPurseCreateItemsRequest { purse, count } = request;
		let out_of_range = CallError::Domain(latest::HostNftPurseCreateItemsError::CountOutOfRange);
		let count = ItemCount::new(count).ok_or(out_of_range)?;
		let purse = PurseId(purse.0);
		let refusals = PurseRefusals {
			denied: latest::HostNftPurseCreateItemsError::Denied,
			other: latest::HostNftPurseCreateItemsError::Other,
		};
		self.require_purse_access(&purses, purse, refusals).await?;
		let items = purses.create_items(purse, count).await.map_err(create_items_failure)?;
		Ok(latest::HostNftPurseCreateItemsResponse { items })
	}

	/// Checks that the caller may use `purse`, asking the user for a grant
	/// when another owner holds it. An unknown purse is refused before anyone
	/// is asked.
	async fn require_purse_access<E>(
		&self,
		purses: &NftPurses,
		purse: PurseId,
		refusals: PurseRefusals<E>,
	) -> Result<(), CallError<E>> {
		let owner = purses.owner_of(purse).await.map_err(owner_lookup_failure)?;
		let unknown = || CallError::Domain((refusals.other)(unknown_purse(purse)));
		let owner = owner.ok_or_else(unknown)?;
		let caller = &self.connection.product.product_id;
		let Some(grant) = nft_purse::read_grant(caller, &owner) else {
			return Ok(());
		};
		self.connection.require_remote_permission(grant, refusals.denied).await
	}
}

/// How one call's error type says a purse was refused.
struct PurseRefusals<E> {
	/// The user did not grant access.
	denied: E,
	/// Any other refusal, such as an unknown purse.
	other: fn(latest::GenericError) -> E,
}

fn unknown_purse(purse: PurseId) -> latest::GenericError {
	latest::GenericError { reason: format!("purse {} does not exist", purse.0) }
}

/// Running out of indices is the product's error. Everything else is the
/// host's.
fn create_items_failure(
	error: AllocateItemsError,
) -> CallError<latest::HostNftPurseCreateItemsError> {
	match error {
		AllocateItemsError::UnknownPurse(_) | AllocateItemsError::IndicesExhausted { .. } => {
			let refusal = latest::GenericError { reason: error.to_string() };
			CallError::Domain(latest::HostNftPurseCreateItemsError::Other(refusal))
		},
		error => CallError::HostFailure { reason: error.to_string() },
	}
}

fn owner_lookup_failure<E>(error: OwnerLookupError) -> CallError<E> {
	CallError::HostFailure { reason: error.to_string() }
}

/// A name over the limit or no free id is the product's error. A database
/// failure is the host's.
fn create_purse_failure(error: CreateProductPurseError) -> CallError<latest::GenericError> {
	if let CreateProductPurseError::Ledger(CreatePurseError::Db(db)) = error {
		return host_failure(db);
	}
	CallError::Domain(latest::GenericError { reason: error.to_string() })
}

fn host_failure<E>(error: DbError) -> CallError<E> {
	CallError::HostFailure { reason: error.to_string() }
}
