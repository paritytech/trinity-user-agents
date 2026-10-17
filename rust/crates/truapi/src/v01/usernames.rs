use parity_scale_codec::{Decode, Encode};

/// Request to find the usernames that start with a text.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostUsernamesSearchRequest {
	/// The start of the usernames, with no `@`: 2 to 64 letters, digits, `.`,
	/// `-` or `_`.
	pub prefix: String,
}

/// The usernames that the host found.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostUsernamesSearchResponse {
	/// At most 20 usernames, with no `@`, in the order of the host.
	pub usernames: Vec<String>,
}

/// Error of a username search.
///
/// A host with no username directory answers `Unsupported` at the framework
/// level rather than through this enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostUsernamesSearchError {
	/// No active session.
	NotConnected,
	/// The prefix breaks a limit.
	InvalidRequest {
		/// Which limit.
		reason: String,
	},
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// Request to add the owner of a username to the contacts of the app.
///
/// The host asks the user first, and adds the person the way it adds a
/// contact itself, so the person is a contact of the app for every product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostUsernamesAddContactRequest {
	/// The username, with no `@`.
	pub username: String,
}

/// How an add of a contact ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum AddContactOutcome {
	/// The host started to add the person: the person is a contact, or is
	/// asked to become one.
	Added,
	/// The person is a contact of the app already.
	AlreadyContact,
	/// The user said no.
	Dismissed,
	/// The network has no such username.
	NotFound,
}

/// Outcome of an add of a contact.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostUsernamesAddContactResponse {
	/// How the add ended.
	pub outcome: AddContactOutcome,
}

/// Error of an add of a contact.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostUsernamesAddContactError {
	/// No active session.
	NotConnected,
	/// The calling execution is not on screen, and is not a Worker handling a
	/// tap from the user, so the host asked nothing.
	NotVisible,
	/// The username breaks a limit.
	InvalidRequest {
		/// Which limit.
		reason: String,
	},
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}
