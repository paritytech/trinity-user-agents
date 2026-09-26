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

/// Request to give the user's chat contacts a profile reference.
///
/// The reference is a bearer capability for everyone the host relays it to.
/// The host stores it as the user's own and never parses it.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct HostProfileDiscloseRequest {
    /// Opaque profile reference, e.g. a Seity contacts reference.
    pub reference: String,
}

impl fmt::Debug for HostProfileDiscloseRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostProfileDiscloseRequest")
            .field("reference", &"[REDACTED]")
            .finish()
    }
}

/// Profile disclosure failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum HostProfileDiscloseError {
    /// The reference is empty, too long, or not printable ASCII.
    InvalidReference,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

/// Profile retraction failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum HostProfileRetractError {
    /// Another product disclosed the reference the host holds.
    NotDiscloser,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

/// Request to show a chat contact's profile in host-owned UI.
///
/// The product names the contact, never a reference: the host looks up the
/// reference that contact's host sent, so the product cannot read, keep or
/// substitute it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct HostProfilePresentContactRequest {
    /// The contact's authenticated root identity, as the chat API names it.
    pub peer_identity: [u8; 32],
}

/// Contact profile presentation failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum HostProfilePresentContactError {
    /// This contact has not shared a profile with the user.
    NotShared,
    /// The host holds a reference it cannot parse.
    InvalidReference,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}
