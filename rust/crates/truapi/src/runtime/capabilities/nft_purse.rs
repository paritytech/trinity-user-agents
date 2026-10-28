//! Product-facing NFT purse capability adapter. Purses live in the core
//! database, which the browser core does not have, so there the served calls
//! answer `Unsupported`, as on a native host that configured no database.
//! Calls no host serves yet answer unavailable everywhere.

#[cfg(not(target_arch = "wasm32"))]
mod served;

#[cfg(target_arch = "wasm32")]
mod browser {
	use truapi::{
		CallContext, CallError, Subscription,
		api::NftPurse,
		versioned::nft_purse::{
			HostNftPurseCreateChequeError, HostNftPurseCreateChequeRequest,
			HostNftPurseCreateChequeResponse, HostNftPurseCreateItemsError,
			HostNftPurseCreateItemsRequest, HostNftPurseCreateItemsResponse,
			HostNftPurseCreatePurseError, HostNftPurseCreatePurseRequest,
			HostNftPurseCreatePurseResponse, HostNftPurseCreateReceiveKeyError,
			HostNftPurseCreateReceiveKeyRequest, HostNftPurseCreateReceiveKeyResponse,
			HostNftPurseDepositError, HostNftPurseDepositRequest, HostNftPurseDepositResponse,
			HostNftPurseListPursesError, HostNftPurseListPursesRequest,
			HostNftPurseListPursesResponse, HostNftPurseObserveOperationStatusError,
			HostNftPurseObserveOperationStatusItem, HostNftPurseObserveOperationStatusRequest,
			HostNftPurseQueryPurseError, HostNftPurseQueryPurseRequest,
			HostNftPurseQueryPurseResponse, HostNftPurseRebalancePurseError,
			HostNftPurseRebalancePurseRequest, HostNftPurseRebalancePurseResponse,
			HostNftPurseRefundError, HostNftPurseRefundRequest, HostNftPurseRefundResponse,
			HostNftPurseRevokeChequeError, HostNftPurseRevokeChequeRequest,
			HostNftPurseRevokeChequeResponse, HostNftPurseStaticReceiveKeyError,
			HostNftPurseStaticReceiveKeyRequest, HostNftPurseStaticReceiveKeyResponse,
			HostNftPurseTransferItemError, HostNftPurseTransferItemRequest,
			HostNftPurseTransferItemResponse,
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

		async fn create_cheque(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseCreateChequeRequest,
		) -> Result<HostNftPurseCreateChequeResponse, CallError<HostNftPurseCreateChequeError>> {
			Err(CallError::Unsupported)
		}

		async fn create_receive_key(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseCreateReceiveKeyRequest,
		) -> Result<
			HostNftPurseCreateReceiveKeyResponse,
			CallError<HostNftPurseCreateReceiveKeyError>,
		> {
			Err(CallError::Unsupported)
		}

		async fn rebalance_purse(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseRebalancePurseRequest,
		) -> Result<HostNftPurseRebalancePurseResponse, CallError<HostNftPurseRebalancePurseError>>
		{
			Err(CallError::Unsupported)
		}

		async fn revoke_cheque(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseRevokeChequeRequest,
		) -> Result<HostNftPurseRevokeChequeResponse, CallError<HostNftPurseRevokeChequeError>> {
			Err(CallError::Unsupported)
		}

		async fn refund(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseRefundRequest,
		) -> Result<HostNftPurseRefundResponse, CallError<HostNftPurseRefundError>> {
			Err(CallError::Unsupported)
		}

		async fn deposit(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseDepositRequest,
		) -> Result<HostNftPurseDepositResponse, CallError<HostNftPurseDepositError>> {
			Err(CallError::Unsupported)
		}

		async fn static_receive_key(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseStaticReceiveKeyRequest,
		) -> Result<
			HostNftPurseStaticReceiveKeyResponse,
			CallError<HostNftPurseStaticReceiveKeyError>,
		> {
			Err(CallError::Unsupported)
		}

		async fn transfer_item(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseTransferItemRequest,
		) -> Result<HostNftPurseTransferItemResponse, CallError<HostNftPurseTransferItemError>> {
			Err(CallError::Unsupported)
		}

		async fn observe_operation_status(
			&self,
			_cx: &CallContext,
			_request: HostNftPurseObserveOperationStatusRequest,
		) -> Subscription<
			HostNftPurseObserveOperationStatusItem,
			CallError<HostNftPurseObserveOperationStatusError>,
		> {
			Subscription::interrupted(CallError::Unsupported)
		}
	}
}
