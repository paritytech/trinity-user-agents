use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

use crate::v01::AvatarRect;
use crate::v02::{OwnAvatarSlot, ProfileContact};

/// A host-rendered avatar placement using peer identities or opaque handles.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostProfilePlaceContactAvatarsRequest {
    /// Width of the drawing surface, from 1 to 16384 units.
    pub surface_width: u32,
    /// Height of the drawing surface, from 1 to 16384 units.
    pub surface_height: u32,
    /// Optional slot for the signed-in user's own disclosed profile.
    pub own: Option<OwnAvatarSlot>,
    /// Complete replacement of the contact slots, at most 64.
    pub slots: Vec<ContactAvatarSlot>,
}

/// Geometry and opaque contact selection for one host-rendered avatar.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ContactAvatarSlot {
    /// Product-chosen id, unique across contact and own slots.
    pub slot: u32,
    /// A peer identity or host-issued handle. Unresolved handles remain blank.
    pub contact: ProfileContact,
    /// Square avatar bounds, from 1 to 1024 units per side.
    pub rect: AvatarRect,
    /// Visible region to which the avatar is clipped.
    pub clip: AvatarRect,
}
