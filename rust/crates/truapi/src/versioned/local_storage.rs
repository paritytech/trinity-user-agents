//! Versioned wrappers for [`LocalStorage`](crate::api::LocalStorage) methods.
//!
//! v0.2 widens the read request from a bare key to a key plus the product whose
//! storage it addresses, and gives the read its own error so it can carry a
//! refusal. A v0.1 caller has no way to name storage other than its own, so
//! upgrading its request means filling the new field with `None`, which is
//! exactly what v0.1 meant.
//!
//! Write and clear keep their v0.1 shape. The manifest grants a read-only
//! `storage` scope and nothing else, so no caller can ever address another
//! product's storage for a write, and there is nothing for those requests to
//! address.

use alloc::string::ToString;

use crate::versioned::{FromLatest, IntoLatest};
use crate::{v01, v02};

truapi_macros::versioned_type! {
    pub enum HostLocalStorageReadRequest {
        V1 => v01::HostLocalStorageReadRequest,
        V2 => v02::HostLocalStorageReadRequest,
    }
    pub enum HostLocalStorageReadResponse {
        V1 => v01::HostLocalStorageReadResponse,
        V2 => v01::HostLocalStorageReadResponse,
    }
    pub enum HostLocalStorageReadError {
        V1 => v01::HostLocalStorageReadError,
        V2 => v02::HostLocalStorageReadError,
    }
    pub enum HostLocalStorageWriteRequest { V1 => v01::HostLocalStorageWriteRequest }
    pub enum HostLocalStorageWriteResponse { V1 }
    pub enum HostLocalStorageWriteError { V1 => v01::HostLocalStorageReadError }
    pub enum HostLocalStorageClearRequest { V1 => v01::HostLocalStorageClearRequest }
    pub enum HostLocalStorageClearResponse { V1 }
    pub enum HostLocalStorageClearError { V1 => v01::HostLocalStorageReadError }
    pub enum HostLocalStorageSubscribeRequest { V1 => v01::HostLocalStorageSubscribeRequest }
    pub enum HostLocalStorageChangeItem { V1 => v01::HostLocalStorageChangeItem }
    pub enum HostLocalStorageSubscribeError { V1 => v01::GenericError }
}

impl IntoLatest for HostLocalStorageReadRequest {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(v01::HostLocalStorageReadRequest { key }) => {
                v02::HostLocalStorageReadRequest { product: None, key }
            }
            Self::V2(latest) => latest,
        }
    }
}

// The read response did not change shape in v0.2. It still gains a V2 variant,
// because a method's version is uniform across its request, response and error
// — without one the generated client would keep every `local_storage.read` call
// pinned to V1 and no product could reach the new field.

impl IntoLatest for HostLocalStorageReadResponse {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(payload) | Self::V2(payload) => payload,
        }
    }
}

impl FromLatest for HostLocalStorageReadResponse {
    fn from_latest(latest: Self::Latest, target: u8) -> Self {
        if target >= 2 {
            Self::V2(latest)
        } else {
            Self::V1(latest)
        }
    }
}

impl IntoLatest for HostLocalStorageReadError {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(v01::HostLocalStorageReadError::Full) => v02::HostLocalStorageReadError::Full,
            Self::V1(v01::HostLocalStorageReadError::Unknown { reason }) => {
                v02::HostLocalStorageReadError::Unknown { reason }
            }
            Self::V2(latest) => latest,
        }
    }
}

impl FromLatest for HostLocalStorageReadError {
    fn from_latest(latest: Self::Latest, target: u8) -> Self {
        if target >= 2 {
            return Self::V2(latest);
        }
        Self::V1(match latest {
            v02::HostLocalStorageReadError::Full => v01::HostLocalStorageReadError::Full,
            v02::HostLocalStorageReadError::Unknown { reason } => {
                v01::HostLocalStorageReadError::Unknown { reason }
            }
            // A v0.1 caller cannot address foreign storage — its request
            // carries no product — so it never provokes this. The downgrade
            // still has to be total, and the variant's own text says it.
            refusal @ v02::HostLocalStorageReadError::AccessNotGranted => {
                v01::HostLocalStorageReadError::Unknown {
                    reason: refusal.to_string(),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versioned::Versioned;

    #[test]
    fn a_v01_request_upgrades_to_the_callers_own_storage() {
        // v0.1 had no way to say anything but "my own storage", so the upgrade
        // must not invent a target.
        let upgraded = HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest {
            key: "k".to_string(),
        })
        .into_latest();
        assert_eq!(
            upgraded,
            v02::HostLocalStorageReadRequest {
                product: None,
                key: "k".to_string(),
            }
        );
    }

    #[test]
    fn a_refusal_reaches_a_v02_peer_as_itself() {
        assert_eq!(
            HostLocalStorageReadError::from_latest(
                v02::HostLocalStorageReadError::AccessNotGranted,
                2
            ),
            HostLocalStorageReadError::V2(v02::HostLocalStorageReadError::AccessNotGranted)
        );
    }

    #[test]
    fn a_refusal_downgrades_to_a_reason_a_v01_peer_can_read() {
        // Unreachable in practice — a v0.1 caller cannot address foreign
        // storage — but the downgrade has to be total.
        for downgraded in [
            HostLocalStorageReadError::from_latest(
                v02::HostLocalStorageReadError::AccessNotGranted,
                1,
            ),
            HostLocalStorageReadError::from_latest(
                v02::HostLocalStorageReadError::AccessNotGranted,
                0,
            ),
        ] {
            assert_eq!(
                downgraded,
                HostLocalStorageReadError::V1(v01::HostLocalStorageReadError::Unknown {
                    reason: v02::HostLocalStorageReadError::AccessNotGranted.to_string(),
                })
            );
        }
    }

    #[test]
    fn a_v01_error_upgrades_without_becoming_a_refusal() {
        // v0.1 has no refusal variant, so nothing it reports may arrive as one.
        for (v1, expected) in [
            (
                v01::HostLocalStorageReadError::Full,
                v02::HostLocalStorageReadError::Full,
            ),
            (
                v01::HostLocalStorageReadError::Unknown {
                    reason: "disk".to_string(),
                },
                v02::HostLocalStorageReadError::Unknown {
                    reason: "disk".to_string(),
                },
            ),
        ] {
            assert_eq!(HostLocalStorageReadError::V1(v1).into_latest(), expected);
        }
    }

    #[test]
    fn quota_exhaustion_keeps_its_own_variant_in_both_directions() {
        // `Full` is the one failure a v0.1 peer can act on differently from a
        // generic error, so it must not collapse into `Unknown`.
        assert_eq!(
            HostLocalStorageReadError::from_latest(v02::HostLocalStorageReadError::Full, 1),
            HostLocalStorageReadError::V1(v01::HostLocalStorageReadError::Full)
        );
        assert_eq!(
            HostLocalStorageReadError::from_latest(v02::HostLocalStorageReadError::Full, 2),
            HostLocalStorageReadError::V2(v02::HostLocalStorageReadError::Full)
        );
    }

    #[test]
    fn v1_keeps_codec_index_zero_so_the_new_version_is_additive() {
        // Not the derive's behaviour under test but the wire's: reordering the
        // variants would renumber V1 and break every deployed v0.1 peer.
        use parity_scale_codec::Encode;

        let v1 = HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest {
            key: "k".to_string(),
        });
        assert_eq!(v1.encode()[0], 0);
        assert_eq!(v1.version(), 1);
        assert_eq!(HostLocalStorageReadRequest::LATEST, 2);
    }

    #[test]
    fn write_and_clear_stay_on_v01() {
        // The `storage` scope is read-only, so no caller can address another
        // product's storage for a write. A V2 on these would advertise reach the
        // manifest cannot grant.
        assert_eq!(HostLocalStorageWriteRequest::LATEST, 1);
        assert_eq!(HostLocalStorageClearRequest::LATEST, 1);
    }
}
