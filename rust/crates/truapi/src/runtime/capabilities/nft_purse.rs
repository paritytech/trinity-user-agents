//! Product-facing NFT purse capability adapter. Purses live in the core
//! database, which the browser core does not have, so there the served calls
//! answer `Unsupported`, as on a native host that configured no database.
//! Calls no host serves yet answer unavailable everywhere.

#[cfg(not(target_arch = "wasm32"))]
mod served;

#[cfg(target_arch = "wasm32")]
mod browser {
	use truapi::{
		CallContext, CallError,
		api::NftPurse,
		versioned::nft_purse::{
			HostNftPurseCreateItemsError, HostNftPurseCreateItemsRequest,
			HostNftPurseCreateItemsResponse, HostNftPurseCreatePurseError,
			HostNftPurseCreatePurseRequest, HostNftPurseCreatePurseResponse,
			HostNftPurseListPursesError, HostNftPurseListPursesRequest,
			HostNftPurseListPursesResponse, HostNftPurseQueryPurseError,
			HostNftPurseQueryPurseRequest, HostNftPurseQueryPurseResponse,
		},
	};

	use crate::runtime::ProductRuntimeHost;

	#[truapi::async_trait]
	impl NftPurse for ProductRuntimeHost {
		async fn create_purse(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseCreatePurseRequest,
		) -> Result<HostNftPurseCreatePurseResponse, CallError<HostNftPurseCreatePurseError>> {
			Err(CallError::Unsupported)
		}

		async fn create_items(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseCreateItemsRequest,
		) -> Result<HostNftPurseCreateItemsResponse, CallError<HostNftPurseCreateItemsError>> {
			Err(CallError::Unsupported)
		}

		async fn query_purse(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseQueryPurseRequest,
		) -> Result<HostNftPurseQueryPurseResponse, CallError<HostNftPurseQueryPurseError>> {
			Err(CallError::Unsupported)
		}

		async fn list_purses(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseListPursesRequest,
		) -> Result<HostNftPurseListPursesResponse, CallError<HostNftPurseListPursesError>> {
			Err(CallError::Unsupported)
		}
	}
}
