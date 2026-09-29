use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

use crate::v01::{AvatarRect, ContactAvatarSlot};

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
