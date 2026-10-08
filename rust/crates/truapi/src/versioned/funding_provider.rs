//! Versioned wrappers for [`FundingProvider`](crate::api::FundingProvider) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostFundingServeSubscribeRequest { V1 }
    pub enum HostFundingServeSubscribeItem { V1 => v01::HostFundingServeSubscribeItem }
    pub enum HostFundingServeSubscribeError { V1 => v01::HostFundingServeSubscribeError }
    pub enum HostFundingReportRequest { V1 => v01::HostFundingReportRequest }
    pub enum HostFundingReportResponse { V1 }
    pub enum HostFundingReportError { V1 => v01::HostFundingReportError }
    pub enum HostFundingPresentFrameRequest { V1 => v01::HostFundingPresentFrameRequest }
    pub enum HostFundingPresentFrameResponse { V1 => v01::HostFundingPresentFrameResponse }
    pub enum HostFundingPresentFrameError { V1 => v01::HostFundingPresentFrameError }
    pub enum HostFundingAnswerQuoteRequest { V1 => v01::HostFundingAnswerQuoteRequest }
    pub enum HostFundingAnswerQuoteResponse { V1 }
    pub enum HostFundingAnswerQuoteError { V1 => v01::HostFundingAnswerQuoteError }
    pub enum HostFundingSaveRequest { V1 => v01::HostFundingSaveRequest }
    pub enum HostFundingSaveResponse { V1 }
    pub enum HostFundingSaveError { V1 => v01::HostFundingSaveError }
}
