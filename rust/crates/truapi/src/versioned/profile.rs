//! Versioned wrappers for [`Profile`](crate::api::Profile) methods.
//!
//! v0.2 of `place_contact_avatars` adds an optional slot for the signed-in
//! user's own avatar. A v0.1 placement upgrades to one with no own slot, which
//! is exactly what v0.1 meant; the response and error keep their v0.1 shape.

use crate::versioned::{FromLatest, IntoLatest};
use crate::{v01, v02};

truapi_macros::versioned_type! {
    pub enum HostProfilePresentRequest { V1 => v01::HostProfilePresentRequest }
    pub enum HostProfilePresentResponse { V1 }
    pub enum HostProfilePresentError { V1 => v01::HostProfilePresentError }
    pub enum HostProfileDiscloseRequest { V1 => v01::HostProfileDiscloseRequest }
    pub enum HostProfileDiscloseResponse { V1 }
    pub enum HostProfileDiscloseError { V1 => v01::HostProfileDiscloseError }
    pub enum HostProfileRetractRequest { V1 }
    pub enum HostProfileRetractResponse { V1 }
    pub enum HostProfileRetractError { V1 => v01::HostProfileRetractError }
    pub enum HostProfilePresentContactRequest { V1 => v01::HostProfilePresentContactRequest }
    pub enum HostProfilePresentContactResponse { V1 }
    pub enum HostProfilePresentContactError { V1 => v01::HostProfilePresentContactError }
    pub enum HostProfilePlaceContactAvatarsRequest {
        V1 => v01::HostProfilePlaceContactAvatarsRequest,
        V2 => v02::HostProfilePlaceContactAvatarsRequest,
    }
    pub enum HostProfilePlaceContactAvatarsResponse { V1, V2 }
    pub enum HostProfilePlaceContactAvatarsError {
        V1 => v01::HostProfilePlaceContactAvatarsError,
        V2 => v01::HostProfilePlaceContactAvatarsError,
    }
    pub enum HostProfileOwnStatusRequest { V1 }
    pub enum HostProfileOwnStatusResponse { V1 => v01::HostProfileOwnStatusResponse }
    pub enum HostProfileOwnStatusError { V1 => v01::HostProfileOwnStatusError }
    pub enum HostProfilePresentOwnRequest { V1 }
    pub enum HostProfilePresentOwnResponse { V1 }
    pub enum HostProfilePresentOwnError { V1 => v01::HostProfilePresentOwnError }
}

impl IntoLatest for HostProfilePlaceContactAvatarsRequest {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(v01::HostProfilePlaceContactAvatarsRequest {
                surface_width,
                surface_height,
                slots,
            }) => v02::HostProfilePlaceContactAvatarsRequest {
                surface_width,
                surface_height,
                own: None,
                slots,
            },
            Self::V2(latest) => latest,
        }
    }
}

// The response and error did not change shape in v0.2. They still gain a V2
// variant, because a method's version is uniform across its request, response
// and error — without one the generated client would keep every placement
// pinned to V1 and no product could reach the own slot.

impl IntoLatest for HostProfilePlaceContactAvatarsResponse {
    fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostProfilePlaceContactAvatarsResponse {
    fn from_latest((): Self::Latest, target: u8) -> Self {
        if target >= 2 { Self::V2 } else { Self::V1 }
    }
}

impl IntoLatest for HostProfilePlaceContactAvatarsError {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(error) | Self::V2(error) => error,
        }
    }
}

impl FromLatest for HostProfilePlaceContactAvatarsError {
    fn from_latest(latest: Self::Latest, target: u8) -> Self {
        if target >= 2 {
            Self::V2(latest)
        } else {
            Self::V1(latest)
        }
    }
}
