use alloc::string::String;
use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

use crate::Bytes32;

/// A contact as a product knows them: 32 bytes and nothing else.
///
/// Its own type rather than a bare [`Bytes32`], because an account id is also
/// 32 bytes. A product that could pass one where the other is expected would
/// build a valid-looking transfer to an address nobody controls. A handle is
/// not an address, and the type is where that is said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct ContactHandle {
    /// The handle's bytes.
    pub bytes: Bytes32,
}

/// How a contact pick ended.
///
/// Distinguishing these matters to a product deciding what to do next: a
/// dismissal is worth retrying, an empty list is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ContactPickOutcome {
    /// The user chose someone.
    ///
    /// `handle` is the same value for this person in every product, and on every
    /// host of this user. It is not an address and cannot be turned into one:
    /// the core resolves it when it builds a transaction, so a product can name
    /// a recipient it never learns the account of.
    Picked {
        /// Stable pseudonym for the chosen contact.
        handle: ContactHandle,
    },
    /// The user closed the picker without choosing.
    Dismissed,
    /// The user has no contacts, so no picker was shown.
    NoContacts,
}

/// Request to open the host's contact picker.
///
/// Carries no arguments: the host owns the overlay, draws it from its own chat
/// contacts, and nothing the product supplies appears in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsPickRequest {}

/// Outcome of a pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsPickResponse {
    /// How the pick ended.
    pub outcome: ContactPickOutcome,
}

/// Error returned by the contact picker.
///
/// Neither a dismissal nor an empty contact list is an error; both are outcomes.
/// A host that serves no picker at all answers `Unsupported` at the framework
/// level rather than through this enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostContactsPickError {
    /// No active session.
    NotConnected,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

/// Selection confirmed in the host's multi-contact picker.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ContactPickManyOutcome {
    /// The user confirmed this complete selection, including an empty one.
    Picked {
        /// Wallet-scoped handles, with duplicates removed.
        handles: Vec<ContactHandle>,
    },
    /// The user closed the picker without confirming a change.
    Dismissed,
    /// There are no contacts to show.
    NoContacts,
}

/// Open the host's picker with the product's current selection.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsPickManyRequest {
    /// At most 256 handles. Duplicates are ignored; an unresolved handle
    /// rejects the entire request rather than changing the selected audience.
    pub selected: Vec<ContactHandle>,
}

/// The user's confirmed selection or reason no selection was made.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsPickManyResponse {
    /// How the picker ended.
    pub outcome: ContactPickManyOutcome,
}

/// Failure before a complete selection can be confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostContactsPickManyError {
    /// No active session, or the session changed while choosing.
    NotConnected,
    /// The selection exceeds the bound or contains an unresolved handle.
    InvalidSelection,
    /// The host could not complete the picker.
    Unknown {
        /// Reason without contact identities or names.
        reason: String,
    },
}

/// Geometry for a host-owned contact name, independent of profile sharing.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ContactLabelSlot {
    /// Product-chosen id, unique within this placement.
    pub slot: u32,
    /// Opaque handle for the contact whose name the host draws.
    pub handle: ContactHandle,
    /// Name bounds in surface units, with sides from 1 to 16384.
    pub rect: super::AvatarRect,
    /// Visible region in surface units; a zero side hides the label.
    pub clip: super::AvatarRect,
}

/// Replace the contact names drawn over a product's surface.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsPlaceLabelsRequest {
    /// Surface width in framebuffer pixels or web viewport CSS pixels, 1 to 16384.
    pub surface_width: u32,
    /// Surface height in the same units, 1 to 16384.
    pub surface_height: u32,
    /// At most 256 slots. Empty clears the previous placement.
    pub slots: Vec<ContactLabelSlot>,
}

/// Acknowledges placement without revealing any contact's name or availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsPlaceLabelsResponse {}

/// Placement failure, never the availability of any individual contact.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Error)
)]
pub enum HostContactsPlaceLabelsError {
    /// The host cannot draw labels over the product surface.
    #[display("contact labels are unsupported")]
    Unsupported,
    /// No active session, or it changed while placing labels.
    #[display("not connected")]
    NotConnected,
    /// The surface, slot count, slot ids or rectangles are invalid.
    #[display("invalid label placement")]
    InvalidPlacement,
    /// The host could not place the labels.
    #[display("{reason}")]
    Unknown {
        /// Reason without contact identities or names.
        reason: String,
    },
}
