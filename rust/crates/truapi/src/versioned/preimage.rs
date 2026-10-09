//! Versioned wrappers for [`Preimage`](crate::api::Preimage) methods.
use alloc::vec::Vec;

use crate::v01;

truapi_macros::versioned_type! {
    pub enum RemotePreimageLookupSubscribeRequest { V1 => v01::RemotePreimageLookupSubscribeRequest }
    pub enum RemotePreimageLookupSubscribeItem { V1 => v01::RemotePreimageLookupSubscribeItem }
    pub enum RemotePreimageLookupSubscribeError { V1 => v01::GenericError }
    pub enum RemotePreimageSubmitRequest { V1 => Vec<u8> }
    pub enum RemotePreimageSubmitResponse { V1 => Vec<u8> }
    pub enum RemotePreimageSubmitError { V1 => v01::PreimageSubmitError }
}
