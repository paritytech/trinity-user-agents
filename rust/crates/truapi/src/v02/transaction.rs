use parity_scale_codec::{Decode, Encode};

/// Transaction creation error.
///
/// v0.2 adds `ChainUnavailable`, so a product can tell a failure worth asking
/// again for from one that is not.
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
	/// The node serving the chain the transaction is for could not be asked
	/// for the current block or its runtime, as while a halted chain is
	/// redialed, so the host built and signed nothing. The same call can
	/// succeed later, but the chain can stay down, so bound the attempts.
	///
	/// A chain the host cannot connect to at all, and a node that never
	/// answers until the call times out, fail as `Unknown`.
	ChainUnavailable {
		/// Human-readable failure reason.
		reason: String,
	},
}
