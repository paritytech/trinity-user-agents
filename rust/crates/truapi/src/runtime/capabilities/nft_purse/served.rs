//! The NFT purse calls a host with a core database serves.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::NftPurse,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		nft_purse::{
			HostNftPurseCreatePurseError, HostNftPurseCreatePurseRequest,
			HostNftPurseCreatePurseResponse, HostNftPurseListPursesError,
			HostNftPurseListPursesRequest, HostNftPurseListPursesResponse,
		},
	},
};

use super::super::from_latest_error;
use crate::{
	nft::CreatePurseError,
	nft_purse::{self, CreateProductPurseError, NftPurses},
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
		let created = purses.create_purse(&self.connection.product_id(), name).await;
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
		if let Some(grant) = nft_purse::read_grant(&self.connection.product_id(), &owner) {
			let denied = latest::HostNftPurseListPursesError::Denied;
			self.connection.require_remote_permission(grant, denied).await?;
		}
		let listed = purses.list_purses(&owner).await.map_err(host_failure)?;
		Ok(latest::HostNftPurseListPursesResponse { purses: listed })
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

fn host_failure<E>(error: DbError) -> CallError<E> {
	CallError::HostFailure { reason: error.to_string() }
}
