use alloc::{string::String, vec::Vec};
use core::fmt;
use parity_scale_codec::{Decode, Encode};

use crate::v01::{AvatarRect, ContactAvatarSlot, ContactHandle};

/// Where a chat product draws avatars the host fills in: its contacts' and,
/// optionally, the signed-in user's own.
///
/// v0.2 adds `own` to the v0.1 placement. A v0.1 placement is this one with no
/// own slot, which is exactly what v0.1 meant. Both kinds live in one
/// placement so a product never has two placements replacing each other's
/// overlay.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostProfilePlaceContactAvatarsRequest {
    /// Width of the product's drawing surface, in the units of every rect:
    /// framebuffer pixels for a PolkaVM product, CSS pixels of its viewport
    /// for a web product. 1 to 16384.
    pub surface_width: u32,
    /// Height of the drawing surface, in the same units. 1 to 16384.
    pub surface_height: u32,
    /// Where the signed-in user's own avatar is drawn, if the product draws
    /// one. The host fills it only when the user has disclosed a profile.
    pub own: Option<OwnAvatarSlot>,
    /// Replaces the product's previous placement entirely; empty clears it.
    /// At most 64, each with its own `slot`, unique across `own` too.
    pub slots: Vec<ContactAvatarSlot>,
}

/// Where the product draws the signed-in user's own avatar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct OwnAvatarSlot {
    /// Product-chosen id, unique within this placement.
    pub slot: u32,
    /// Bounding box of the avatar circle: square, 1 to 1024 units a side.
    pub rect: AvatarRect,
    /// Visible region the avatar is cut to.
    pub clip: AvatarRect,
}

/// Recipients of one profile reference. Multiple audiences form a union.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ProfileAudience {
    /// Every ready Chat App, with profiles scoped to that App.
    ChatApps,
    /// Contacts of this Chat App, with profiles scoped to that App.
    App {
        /// Canonical product identifier.
        product_id: String,
    },
    /// Selected contacts, with profiles available in any receiving App.
    Contacts {
        /// Opaque handles returned by the host's contact picker, at most 4096
        /// across the request.
        handles: Vec<ContactHandle>,
    },
}

/// Replace the user's disclosed reference and its complete set of audiences.
///
/// An empty audience retains the user's own profile but withdraws all delivery
/// grants. A contact handle that no longer resolves rejects the whole request.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostProfileDiscloseRequest {
    /// Opaque bearer reference, retained and relayed only by the host.
    pub reference: String,
    /// Independent grants for this reference, at most 64.
    pub audiences: Vec<ProfileAudience>,
}

impl fmt::Debug for HostProfileDiscloseRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostProfileDiscloseRequest")
            .field("reference", &"[REDACTED]")
            .field("audiences", &"[REDACTED]")
            .finish()
    }
}

/// A contact named without exposing a handle's account to the product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ProfileContact {
    /// An authenticated Chat network identity already known to the product.
    Peer {
        /// The contact's identity account, not a product device account.
        peer_identity: [u8; 32],
    },
    /// An opaque host-issued contact selection.
    Handle {
        /// A handle returned by the contact picker.
        handle: ContactHandle,
    },
}

/// Ask the host to present a contact's available profile.
///
/// Unknown handles, absent profiles and presentation failures return success,
/// without disclosing whether the contact shares a profile.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostProfilePresentContactRequest {
    /// The contact whose profile the host may present.
    pub contact: ProfileContact,
}
