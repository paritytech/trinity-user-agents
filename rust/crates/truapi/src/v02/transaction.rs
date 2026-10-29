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
	/// The chain the call depends on stayed unreachable, so the host built,
	/// sent and signed nothing. Either the node serving the transaction's
	/// chain could not be asked for the current block or its runtime, as
	/// while a halted chain is redialed, or gave no answer in time, or a
	/// request to the signing host never reached it, because the chain it
	/// travels over stayed halted or reached no peer. The same call can
	/// succeed later, but the chain can stay down, so bound the attempts.
	///
	/// A chain the host cannot connect to at all, a provider that cannot
	/// follow the chain, a runtime the host cannot use, and a request to the
	/// signing host that failed after it reached a peer fail as `Unknown`.
	ChainUnavailable {
		/// Human-readable failure reason.
		reason: String,
	},
}
