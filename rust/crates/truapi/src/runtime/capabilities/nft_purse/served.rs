//! The NFT purse calls a host with a core database serves.

mod cheques;
mod operations;

use tracing::instrument;
use truapi::{
	CallContext, CallError, Subscription,
	api::NftPurse,
	latest,
	versioned::nft_purse::{
		HostNftPurseCreateChequeError, HostNftPurseCreateChequeRequest,
		HostNftPurseCreateChequeResponse, HostNftPurseCreateItemsError,
		HostNftPurseCreateItemsRequest, HostNftPurseCreateItemsResponse,
		HostNftPurseCreatePurseError, HostNftPurseCreatePurseRequest,
		HostNftPurseCreatePurseResponse, HostNftPurseCreateReceiveKeyError,
		HostNftPurseCreateReceiveKeyRequest, HostNftPurseCreateReceiveKeyResponse,
		HostNftPurseDepositError, HostNftPurseDepositRequest, HostNftPurseDepositResponse,
		HostNftPurseListPursesError, HostNftPurseListPursesRequest, HostNftPurseListPursesResponse,
		HostNftPurseObserveOperationStatusError, HostNftPurseObserveOperationStatusItem,
		HostNftPurseObserveOperationStatusRequest, HostNftPurseQueryPurseError,
		HostNftPurseQueryPurseRequest, HostNftPurseQueryPurseResponse,
		HostNftPurseRebalancePurseError, HostNftPurseRebalancePurseRequest,
		HostNftPurseRebalancePurseResponse, HostNftPurseRevokeChequeError,
		HostNftPurseRevokeChequeRequest, HostNftPurseRevokeChequeResponse,
		HostNftPurseStaticReceiveKeyError, HostNftPurseStaticReceiveKeyRequest,
		HostNftPurseStaticReceiveKeyResponse, HostNftPurseTransferItemError,
		HostNftPurseTransferItemRequest, HostNftPurseTransferItemResponse,
	},
};

use crate::{
	nft::{AllocateItemsError, CreatePurseError, PurseId, UnknownPurse},
	nft_purse::{self, CreateProductPurseError, ItemCount, NftPurses, PageSize, PurseOwner},
	runtime::{ProductConnection, ProductRuntimeHost, versioning::version},
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
		version(request).exec(|request| self.create_nft_purse(request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.create_items"))]
	async fn create_items(
		&self,
		_cx: &CallContext,
		request: HostNftPurseCreateItemsRequest,
	) -> Result<HostNftPurseCreateItemsResponse, CallError<HostNftPurseCreateItemsError>> {
		version(request).exec(|request| self.create_nft_items(request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.query_purse"))]
	async fn query_purse(
		&self,
		_cx: &CallContext,
		request: HostNftPurseQueryPurseRequest,
	) -> Result<HostNftPurseQueryPurseResponse, CallError<HostNftPurseQueryPurseError>> {
		version(request).exec(|request| self.query_nft_purse(request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.list_purses"))]
	async fn list_purses(
		&self,
		_cx: &CallContext,
		request: HostNftPurseListPursesRequest,
	) -> Result<HostNftPurseListPursesResponse, CallError<HostNftPurseListPursesError>> {
		version(request).exec(|request| self.list_nft_purses(request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.create_receive_key"))]
	async fn create_receive_key(
		&self,
		_cx: &CallContext,
		request: HostNftPurseCreateReceiveKeyRequest,
	) -> Result<HostNftPurseCreateReceiveKeyResponse, CallError<HostNftPurseCreateReceiveKeyError>>
	{
		version(request).exec(|request| self.create_nft_receive_key(request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.create_cheque"))]
	async fn create_cheque(
		&self,
		cx: &CallContext,
		request: HostNftPurseCreateChequeRequest,
	) -> Result<HostNftPurseCreateChequeResponse, CallError<HostNftPurseCreateChequeError>> {
		version(request).exec(|request| self.create_nft_cheque(cx, request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.revoke_cheque"))]
	async fn revoke_cheque(
		&self,
		_cx: &CallContext,
		request: HostNftPurseRevokeChequeRequest,
	) -> Result<HostNftPurseRevokeChequeResponse, CallError<HostNftPurseRevokeChequeError>> {
		version(request).exec(|request| self.revoke_nft_cheque(request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.deposit"))]
	async fn deposit(
		&self,
		_cx: &CallContext,
		request: HostNftPurseDepositRequest,
	) -> Result<HostNftPurseDepositResponse, CallError<HostNftPurseDepositError>> {
		version(request).exec(|request| self.deposit_nft_cheque(request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.static_receive_key"))]
	async fn static_receive_key(
		&self,
		_cx: &CallContext,
		request: HostNftPurseStaticReceiveKeyRequest,
	) -> Result<HostNftPurseStaticReceiveKeyResponse, CallError<HostNftPurseStaticReceiveKeyError>>
	{
		version(request).exec(|_| async { self.static_nft_receive_key() }).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.rebalance_purse"))]
	async fn rebalance_purse(
		&self,
		cx: &CallContext,
		request: HostNftPurseRebalancePurseRequest,
	) -> Result<HostNftPurseRebalancePurseResponse, CallError<HostNftPurseRebalancePurseError>> {
		version(request).exec(|request| self.rebalance_nft_purse(cx, request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.transfer_item"))]
	async fn transfer_item(
		&self,
		cx: &CallContext,
		request: HostNftPurseTransferItemRequest,
	) -> Result<HostNftPurseTransferItemResponse, CallError<HostNftPurseTransferItemError>> {
		version(request).exec(|request| self.transfer_nft_item(cx, request)).await
	}

	#[instrument(skip_all, fields(runtime.method = "nft_purse.observe_operation_status"))]
	async fn observe_operation_status(
		&self,
		_cx: &CallContext,
		request: HostNftPurseObserveOperationStatusRequest,
	) -> Subscription<
		HostNftPurseObserveOperationStatusItem,
		CallError<HostNftPurseObserveOperationStatusError>,
	> {
		version(request)
			.exec(|request| async move { self.observe_nft_operation(request) })
			.await
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
		let created = purses.create_purse(self.connection.product_id(), name).await;
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
		if let Some(grant) = nft_purse::read_grant(self.connection.product_id(), &owner) {
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
		self.require_purse_access(&purses, purse, CREATE_ITEMS_REFUSALS).await?;
		let items = purses.create_items(purse, count).await.map_err(create_items_failure)?;
		Ok(latest::HostNftPurseCreateItemsResponse { items })
	}

	async fn query_nft_purse(
		&self,
		request: latest::HostNftPurseQueryPurseRequest,
	) -> Result<
		latest::HostNftPurseQueryPurseResponse,
		CallError<latest::HostNftPurseQueryPurseError>,
	> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let latest::HostNftPurseQueryPurseRequest { purse, page, page_size } = request;
		let out_of_bounds =
			CallError::Domain(latest::HostNftPurseQueryPurseError::PageSizeOutOfBounds);
		let size = PageSize::new(page_size).ok_or(out_of_bounds)?;
		let purse = PurseId(purse.0);
		self.require_purse_access(&purses, purse, QUERY_PURSE_REFUSALS).await?;
		let page = purses.query_purse(purse, page, size).await.map_err(host_failure)?;
		Ok(latest::HostNftPurseQueryPurseResponse { page })
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
		let owner = purses.owner_of(purse).await.map_err(host_failure)?;
		let owner = owner.ok_or_else(|| refusals.unknown_purse(purse))?;
		let Some(grant) = nft_purse::read_grant(self.connection.product_id(), &owner) else {
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

impl<E> PurseRefusals<E> {
	/// The refusal of a purse that does not exist.
	fn unknown_purse(&self, purse: PurseId) -> CallError<E> {
		let reason = latest::GenericError { reason: UnknownPurse(purse).to_string() };
		CallError::Domain((self.other)(reason))
	}
}

const CREATE_ITEMS_REFUSALS: PurseRefusals<latest::HostNftPurseCreateItemsError> = PurseRefusals {
	denied: latest::HostNftPurseCreateItemsError::Denied,
	other: latest::HostNftPurseCreateItemsError::Other,
};

const QUERY_PURSE_REFUSALS: PurseRefusals<latest::HostNftPurseQueryPurseError> = PurseRefusals {
	denied: latest::HostNftPurseQueryPurseError::Denied,
	other: latest::HostNftPurseQueryPurseError::Other,
};

/// An unknown purse or running out of indices is the product's error.
/// Everything else is the host's.
fn create_items_failure(
	error: AllocateItemsError,
) -> CallError<latest::HostNftPurseCreateItemsError> {
	match error {
		AllocateItemsError::UnknownPurse(_) | AllocateItemsError::IndicesExhausted { .. } => {
			let refusal = latest::GenericError { reason: error.to_string() };
			CallError::Domain(latest::HostNftPurseCreateItemsError::Other(refusal))
		},
		error => host_failure(error),
	}
}

/// A name over the limit or no free id is the product's error. A database
/// failure is the host's.
fn create_purse_failure(error: CreateProductPurseError) -> CallError<latest::GenericError> {
	if let CreateProductPurseError::Ledger(CreatePurseError::Db(db)) = error {
		return host_failure(db);
	}
	CallError::Domain(latest::GenericError { reason: error.to_string() })
}

fn host_failure<E>(error: impl core::fmt::Display) -> CallError<E> {
	CallError::HostFailure { reason: error.to_string() }
}
