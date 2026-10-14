use derive_more::Display;
use parity_scale_codec::{Decode, Encode};

/// Request to begin a pending operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostWorkerBeginOperationRequest {
	/// Optional label for host logs and UI.
	pub label: Option<String>,
}

/// Response carrying the id of a newly begun operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostWorkerBeginOperationResponse {
	/// Id to pass to `end_operation`.
	pub id: u32,
}

/// Request to end a pending operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostWorkerEndOperationRequest {
	/// Id returned by `begin_operation`.
	pub id: u32,
}

/// Pending-operation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum HostWorkerOperationError {
	/// The product is already at the host's per-product limit of open
	/// operations.
	#[display("too many open operations")]
	TooManyOpen,
	/// Catch-all host failure.
	#[display("{reason}")]
	Unknown {
		/// Human-readable failure reason.
		reason: String,
	},
}
