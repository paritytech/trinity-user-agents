use parity_scale_codec::{Decode, Encode};

use crate::Bytes32;

/// A contact as a product knows them: 32 bytes and nothing else.
///
/// Its own type rather than a bare [`Bytes32`], because an account id is also
/// 32 bytes. A product that could pass one where the other is expected would
/// build a valid-looking transfer to an address nobody controls. A handle is
/// not an address, and the type is where that is said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
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
