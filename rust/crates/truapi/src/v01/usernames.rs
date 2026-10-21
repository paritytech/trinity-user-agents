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
