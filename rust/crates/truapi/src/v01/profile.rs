use alloc::string::String;
use alloc::vec::Vec;
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
    /// The user declined to let this product disclose a profile to their
    /// chat contacts.
    PermissionDenied,
    /// No user is signed in, so there are no contacts to disclose to.
    NotConnected,
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
    /// No user is signed in.
    NotConnected,
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
    /// No user is signed in.
    NotConnected,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

/// Where a chat product draws contact avatars, so the host can draw the
/// photo and mood ring each contact shared over them, on its own layer.
///
/// The product sends geometry only. The host decides which slots it can fill
/// and never says which, so the product cannot learn who shared a profile.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct HostProfilePlaceContactAvatarsRequest {
    /// Width of the product's drawing surface, in the units of every rect:
    /// framebuffer pixels for a PolkaVM product, CSS pixels of its viewport
    /// for a web product. 1 to 16384.
    pub surface_width: u32,
    /// Height of the drawing surface, in the same units. 1 to 16384.
    pub surface_height: u32,
    /// Replaces the product's previous placement entirely; empty clears it.
    /// At most 64, each with its own `slot`.
    pub slots: Vec<ContactAvatarSlot>,
}

/// One avatar the product draws for a chat contact.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ContactAvatarSlot {
    /// Product-chosen id, stable for one on-screen avatar (a list row, a
    /// header). The host uses it only to keep what it draws stable across
    /// updates.
    pub slot: u32,
    /// The contact's authenticated root identity, as the chat API names it.
    pub peer_identity: [u8; 32],
    /// Bounding box of the avatar circle: square, 1 to 1024 units a side.
    pub rect: AvatarRect,
    /// Visible region the avatar is cut to, such as the scroll area.
    pub clip: AvatarRect,
}

/// A rectangle in surface units, relative to the surface's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AvatarRect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// Contact avatar placement failure. Says nothing about any one slot.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum HostProfilePlaceContactAvatarsError {
    /// This host cannot draw over the product's surface.
    Unsupported,
    /// No user is signed in.
    NotConnected,
    /// Catch-all, including a malformed placement.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}
