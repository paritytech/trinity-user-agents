use parity_scale_codec::{Decode, Encode};

/// Transaction creation error. v0.2 adds `ChainUnavailable`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostCreateTransactionError {
	/// Payload could not be deserialized.
	FailedToDecode,
	/// User rejected.
	Rejected,
	/// Unsupported payload version or extension.
	NotSupported {
		/// Unsupported payload or extension reason.
		reason: String,
	},
	/// Not authenticated.
	PermissionDenied,
	/// Catch-all.
	Unknown {
		/// Human-readable failure reason.
		reason: String,
	},
	/// A declared contact handle names nobody this host has a contact for, or
	/// does not appear in the call it was declared for, or a handle appears in
	/// the call without being declared. One refusal for all three, because
	/// telling them apart would say whether a handle is current.
	UnknownContact,
	/// The chain stayed unreachable, such as during a halt, so nothing was
	/// built, sent or signed. A later call can succeed, but bound the
	/// attempts. Failures a retry cannot fix, such as a chain the host does
	/// not serve, are `Unknown`.
	ChainUnavailable {
		/// Human-readable failure reason.
		reason: String,
	},
}
