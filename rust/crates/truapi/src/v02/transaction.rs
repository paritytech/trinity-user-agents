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
	/// A contact handle is unknown, missing from the call, or used without
	/// being declared. One error for all three, so it does not reveal whether
	/// a handle is current.
	UnknownContact,
	/// The chain stayed unreachable, such as during a halt, so nothing was
	/// signed. Safe to retry, with a bound on the attempts.
	ChainUnavailable {
		/// Human-readable failure reason.
		reason: String,
	},
}
