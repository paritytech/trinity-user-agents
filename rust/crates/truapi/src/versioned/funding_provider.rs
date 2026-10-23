//! Versioned wrappers for [`FundingProvider`](crate::api::FundingProvider) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum ProductFundingQuoteRequest { V1 => v01::ProductFundingQuoteRequest }
	pub enum ProductFundingQuoteItem { V1 => v01::FundingQuoteAnswer }
	pub enum ProductFundingQuoteError { V1 => v01::GenericError }
	pub enum ProductFundingAssignRequest { V1 => v01::ProductFundingAssignRequest }
	pub enum ProductFundingAssignItem { V1 }
	pub enum ProductFundingAssignError { V1 => v01::GenericError }
	pub enum ProductFundingCancelRequest { V1 => v01::ProductFundingCancelRequest }
	pub enum ProductFundingCancelItem { V1 => v01::FundingCancelAnswer }
	pub enum ProductFundingCancelError { V1 => v01::GenericError }
	pub enum ProductFundingSettleCancelRequest { V1 => v01::ProductFundingSettleCancelRequest }
	pub enum ProductFundingSettleCancelItem { V1 }
	pub enum ProductFundingSettleCancelError { V1 => v01::GenericError }
	pub enum HostFundingReportRequest { V1 => v01::HostFundingReportRequest }
	pub enum HostFundingReportResponse { V1 }
	pub enum HostFundingReportError { V1 => v01::HostFundingReportError }
	pub enum HostFundingPresentFrameRequest { V1 => v01::HostFundingPresentFrameRequest }
	pub enum HostFundingPresentFrameResponse { V1 => v01::HostFundingPresentFrameResponse }
	pub enum HostFundingPresentFrameError { V1 => v01::HostFundingPresentFrameError }
}
