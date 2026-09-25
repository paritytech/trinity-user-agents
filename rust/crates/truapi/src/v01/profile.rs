use alloc::string::String;
use core::fmt;
use parity_scale_codec::{Decode, Encode};

/// Request to show a profile the calling product references in host-owned UI.
///
/// The reference is a bearer capability: whoever holds it can read the profile
/// it names. The host resolves and renders it itself, so profile bytes, the
/// avatar image included, never reach the product.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct HostProfilePresentRequest {
    /// Opaque profile reference, e.g. a Seity `<cid>#<key>` blob reference.
    pub reference: String,
}

impl fmt::Debug for HostProfilePresentRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostProfilePresentRequest")
            .field("reference", &"[REDACTED]")
            .finish()
    }
}

/// Profile presentation failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum HostProfilePresentError {
    /// The reference is malformed or names a format this host cannot open.
    InvalidReference,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}
