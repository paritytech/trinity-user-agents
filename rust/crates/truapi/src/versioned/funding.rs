//! Versioned wrappers for [`Funding`](crate::api::Funding) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostFundingRequest { V1 => v01::HostFundingRequest }
    pub enum HostFundingResponse { V1 => v01::HostFundingResponse }
    pub enum HostFundingError { V1 => v01::HostFundingError }
    pub enum HostFundingStatusSubscribeRequest { V1 => v01::HostFundingStatusSubscribeRequest }
    pub enum HostFundingStatusSubscribeItem { V1 => v01::HostFundingStatusSubscribeItem }
    pub enum HostFundingStatusSubscribeError { V1 => v01::HostFundingStatusSubscribeError }
}
