//! Versioned wrappers for [`Profile`](crate::api::Profile) methods.
//! Each method keeps its original payload decodable while upgrading into the
//! current audience and contact-selector model.

use crate::versioned::{FromLatest, IntoLatest};
use crate::{v01, v02, v03};

truapi_macros::versioned_type! {
    pub enum HostProfilePresentRequest { V1 => v01::HostProfilePresentRequest }
    pub enum HostProfilePresentResponse { V1 }
    pub enum HostProfilePresentError { V1 => v01::HostProfilePresentError }
    pub enum HostProfileDiscloseRequest {
        V1 => v01::HostProfileDiscloseRequest,
        V2 => v02::HostProfileDiscloseRequest,
    }
    pub enum HostProfileDiscloseResponse { V1, V2 }
    pub enum HostProfileDiscloseError {
        V1 => v01::HostProfileDiscloseError,
        V2 => v01::HostProfileDiscloseError,
    }
    pub enum HostProfileRetractRequest { V1 }
    pub enum HostProfileRetractResponse { V1 }
    pub enum HostProfileRetractError { V1 => v01::HostProfileRetractError }
    pub enum HostProfilePresentContactRequest {
        V1 => v01::HostProfilePresentContactRequest,
        V2 => v02::HostProfilePresentContactRequest,
    }
    pub enum HostProfilePresentContactResponse { V1, V2 }
    pub enum HostProfilePresentContactError {
        V1 => v01::HostProfilePresentContactError,
        V2 => v01::HostProfilePresentContactError,
    }
    pub enum HostProfilePlaceContactAvatarsRequest {
        V1 => v01::HostProfilePlaceContactAvatarsRequest,
        V2 => v02::HostProfilePlaceContactAvatarsRequest,
        V3 => v03::HostProfilePlaceContactAvatarsRequest,
    }
    pub enum HostProfilePlaceContactAvatarsResponse { V1, V2, V3 }
    pub enum HostProfilePlaceContactAvatarsError {
        V1 => v01::HostProfilePlaceContactAvatarsError,
        V2 => v01::HostProfilePlaceContactAvatarsError,
        V3 => v01::HostProfilePlaceContactAvatarsError,
    }
    pub enum HostProfileOwnStatusRequest { V1 }
    pub enum HostProfileOwnStatusResponse { V1 => v01::HostProfileOwnStatusResponse }
    pub enum HostProfileOwnStatusError { V1 => v01::HostProfileOwnStatusError }
    pub enum HostProfilePresentOwnRequest { V1 }
    pub enum HostProfilePresentOwnResponse { V1 }
    pub enum HostProfilePresentOwnError { V1 => v01::HostProfilePresentOwnError }
}

impl IntoLatest for HostProfileDiscloseRequest {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(request) => v02::HostProfileDiscloseRequest {
                reference: request.reference,
                audiences: alloc::vec![v02::ProfileAudience::ChatApps],
            },
            Self::V2(request) => request,
        }
    }
}

impl IntoLatest for HostProfilePresentContactRequest {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(request) => v02::HostProfilePresentContactRequest {
                contact: v02::ProfileContact::Peer {
                    peer_identity: request.peer_identity,
                },
            },
            Self::V2(request) => request,
        }
    }
}

impl IntoLatest for HostProfileDiscloseResponse {
    fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostProfileDiscloseResponse {
    fn from_latest((): Self::Latest, target: u8) -> Self {
        if target >= 2 { Self::V2 } else { Self::V1 }
    }
}

impl IntoLatest for HostProfileDiscloseError {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(error) | Self::V2(error) => error,
        }
    }
}

impl FromLatest for HostProfileDiscloseError {
    fn from_latest(error: Self::Latest, target: u8) -> Self {
        if target >= 2 {
            Self::V2(error)
        } else {
            Self::V1(error)
        }
    }
}

impl IntoLatest for HostProfilePresentContactResponse {
    fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostProfilePresentContactResponse {
    fn from_latest((): Self::Latest, target: u8) -> Self {
        if target >= 2 { Self::V2 } else { Self::V1 }
    }
}

impl IntoLatest for HostProfilePresentContactError {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(error) | Self::V2(error) => error,
        }
    }
}

impl FromLatest for HostProfilePresentContactError {
    fn from_latest(error: Self::Latest, target: u8) -> Self {
        if target >= 2 {
            Self::V2(error)
        } else {
            Self::V1(error)
        }
    }
}

impl IntoLatest for HostProfilePlaceContactAvatarsRequest {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(v01::HostProfilePlaceContactAvatarsRequest {
                surface_width,
                surface_height,
                slots,
            }) => v03::HostProfilePlaceContactAvatarsRequest {
                surface_width,
                surface_height,
                own: None,
                slots: slots.into_iter().map(upgrade_slot).collect(),
            },
            Self::V2(request) => v03::HostProfilePlaceContactAvatarsRequest {
                surface_width: request.surface_width,
                surface_height: request.surface_height,
                own: request.own,
                slots: request.slots.into_iter().map(upgrade_slot).collect(),
            },
            Self::V3(latest) => latest,
        }
    }
}

fn upgrade_slot(slot: v01::ContactAvatarSlot) -> v03::ContactAvatarSlot {
    v03::ContactAvatarSlot {
        slot: slot.slot,
        contact: v02::ProfileContact::Peer {
            peer_identity: slot.peer_identity,
        },
        rect: slot.rect,
        clip: slot.clip,
    }
}

impl IntoLatest for HostProfilePlaceContactAvatarsResponse {
    fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostProfilePlaceContactAvatarsResponse {
    fn from_latest((): Self::Latest, target: u8) -> Self {
        if target >= 3 {
            Self::V3
        } else if target == 2 {
            Self::V2
        } else {
            Self::V1
        }
    }
}

impl IntoLatest for HostProfilePlaceContactAvatarsError {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(error) | Self::V2(error) | Self::V3(error) => error,
        }
    }
}

impl FromLatest for HostProfilePlaceContactAvatarsError {
    fn from_latest(latest: Self::Latest, target: u8) -> Self {
        if target >= 3 {
            Self::V3(latest)
        } else if target == 2 {
            Self::V2(latest)
        } else {
            Self::V1(latest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::{DecodeAll, Encode};

    #[test]
    fn original_disclose_bytes_keep_the_all_chat_audience() {
        let request = HostProfileDiscloseRequest::decode_all(&mut &[0, 4, b'x'][..]).unwrap();
        assert_eq!(
            request.into_latest(),
            v02::HostProfileDiscloseRequest {
                reference: "x".into(),
                audiences: alloc::vec![v02::ProfileAudience::ChatApps],
            },
        );
        assert_eq!(
            HostProfileDiscloseRequest::V2(v02::HostProfileDiscloseRequest {
                reference: "x".into(),
                audiences: alloc::vec![],
            })
            .encode(),
            alloc::vec![1, 4, b'x', 0],
            "an explicitly empty audience must not acquire the legacy grant",
        );
    }

    #[test]
    fn original_contact_bytes_remain_a_peer_not_a_handle() {
        let bytes = (0u8, [7u8; 32]).encode();
        let request = HostProfilePresentContactRequest::decode_all(&mut &bytes[..]).unwrap();
        assert_eq!(
            request.into_latest().contact,
            v02::ProfileContact::Peer {
                peer_identity: [7; 32]
            },
        );
        let bytes = (1u8, 1u8, [7u8; 32]).encode();
        let request = HostProfilePresentContactRequest::decode_all(&mut &bytes[..]).unwrap();
        assert_eq!(
            request.into_latest().contact,
            v02::ProfileContact::Handle {
                handle: v01::ContactHandle { bytes: [7; 32] }
            },
        );
    }

    #[test]
    fn old_avatar_layouts_preserve_geometry_and_the_optional_own_slot() {
        let rect = v01::AvatarRect {
            x: -1,
            y: 20,
            width: 44,
            height: 44,
        };
        let slot = v01::ContactAvatarSlot {
            slot: 2,
            peer_identity: [7; 32],
            rect,
            clip: rect,
        };
        let own = v02::OwnAvatarSlot {
            slot: 1,
            rect,
            clip: rect,
        };
        for (bytes, expected_own) in [
            (
                (0u8, 360u32, 640u32, alloc::vec![slot.clone()]).encode(),
                None,
            ),
            (
                (1u8, 360u32, 640u32, Some(own), alloc::vec![slot]).encode(),
                Some(own),
            ),
        ] {
            let request =
                HostProfilePlaceContactAvatarsRequest::decode_all(&mut &bytes[..]).unwrap();
            assert_eq!(
                request.into_latest(),
                v03::HostProfilePlaceContactAvatarsRequest {
                    surface_width: 360,
                    surface_height: 640,
                    own: expected_own,
                    slots: alloc::vec![v03::ContactAvatarSlot {
                        slot: 2,
                        contact: v02::ProfileContact::Peer {
                            peer_identity: [7; 32]
                        },
                        rect,
                        clip: rect,
                    }],
                }
            );
        }
    }
}
