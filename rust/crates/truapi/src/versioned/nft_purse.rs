//! Versioned wrappers for [`NftPurse`](crate::api::NftPurse) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostNftPurseCreatePurseRequest { V1 => v01::HostNftPurseCreatePurseRequest }
	pub enum HostNftPurseCreatePurseResponse { V1 => v01::HostNftPurseCreatePurseResponse }
	pub enum HostNftPurseCreatePurseError { V1 => v01::GenericError }
	pub enum HostNftPurseCreateItemsRequest { V1 => v01::HostNftPurseCreateItemsRequest }
	pub enum HostNftPurseCreateItemsResponse { V1 => v01::HostNftPurseCreateItemsResponse }
	pub enum HostNftPurseCreateItemsError { V1 => v01::NftPurseCreateItemsError }
	pub enum HostNftPurseQueryPurseRequest { V1 => v01::HostNftPurseQueryPurseRequest }
	pub enum HostNftPurseQueryPurseResponse { V1 => v01::HostNftPurseQueryPurseResponse }
	pub enum HostNftPurseQueryPurseError { V1 => v01::NftPurseQueryPurseError }
	pub enum HostNftPurseListPursesRequest { V1 => v01::HostNftPurseListPursesRequest }
	pub enum HostNftPurseListPursesResponse { V1 => v01::HostNftPurseListPursesResponse }
	pub enum HostNftPurseListPursesError { V1 => v01::NftPurseListPursesError }
	pub enum HostNftPurseRebalancePurseRequest { V1 => v01::HostNftPurseRebalancePurseRequest }
	pub enum HostNftPurseRebalancePurseResponse { V1 }
	pub enum HostNftPurseRebalancePurseError { V1 => v01::NftPurseRebalancePurseError }
	pub enum HostNftPurseCreateReceiveKeyRequest { V1 => v01::HostNftPurseCreateReceiveKeyRequest }
	pub enum HostNftPurseCreateReceiveKeyResponse { V1 => v01::HostNftPurseCreateReceiveKeyResponse }
	pub enum HostNftPurseCreateReceiveKeyError { V1 => v01::NftPurseCreateReceiveKeyError }
	pub enum HostNftPurseListenForPaymentRequest { V1 => v01::HostNftPurseListenForPaymentRequest }
	pub enum HostNftPurseListenForPaymentItem { V1 => v01::HostNftPurseListenForPaymentItem }
	pub enum HostNftPurseListenForPaymentError { V1 => v01::NftPurseListenForPaymentError }
	pub enum HostNftPurseCreateChequeRequest { V1 => v01::HostNftPurseCreateChequeRequest }
	pub enum HostNftPurseCreateChequeResponse { V1 => v01::HostNftPurseCreateChequeResponse }
	pub enum HostNftPurseCreateChequeError { V1 => v01::NftPurseCreateChequeError }
	pub enum HostNftPurseRevokeChequeRequest { V1 => v01::HostNftPurseRevokeChequeRequest }
	pub enum HostNftPurseRevokeChequeResponse { V1 }
	pub enum HostNftPurseRevokeChequeError { V1 => v01::NftPurseRevokeChequeError }
	pub enum HostNftPurseDepositRequest { V1 => v01::HostNftPurseDepositRequest }
	pub enum HostNftPurseDepositResponse { V1 }
	pub enum HostNftPurseDepositError { V1 => v01::NftPurseDepositError }
	pub enum HostNftPurseRefundRequest { V1 => v01::HostNftPurseRefundRequest }
	pub enum HostNftPurseRefundResponse { V1 => v01::HostNftPurseRefundResponse }
	pub enum HostNftPurseRefundError { V1 => v01::NftPurseRefundError }
	pub enum HostNftPurseTransferItemRequest { V1 => v01::HostNftPurseTransferItemRequest }
	pub enum HostNftPurseTransferItemResponse { V1 }
	pub enum HostNftPurseTransferItemError { V1 => v01::NftPurseTransferItemError }
	pub enum HostNftPurseObserveOperationStatusRequest { V1 => v01::HostNftPurseObserveOperationStatusRequest }
	pub enum HostNftPurseObserveOperationStatusItem { V1 => v01::NftPurseStatus }
	pub enum HostNftPurseObserveOperationStatusError { V1 => v01::NftPurseObserveOperationStatusError }
	pub enum HostNftPurseStaticReceiveKeyRequest { V1 => v01::HostNftPurseStaticReceiveKeyRequest }
	pub enum HostNftPurseStaticReceiveKeyResponse { V1 => v01::HostNftPurseStaticReceiveKeyResponse }
	pub enum HostNftPurseStaticReceiveKeyError { V1 => v01::NftPurseStaticReceiveKeyError }
}
