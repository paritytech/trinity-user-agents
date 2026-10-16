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

/// One group of statement-store topics whose new statements wake the worker,
/// and the chat room the wake goes to.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct WorkerWakeWatch {
	/// Product-chosen id of the watch. The wake event of the room names it.
	pub id: String,
	/// Room of the product that receives `ChatRoomEvent::Woken`.
	pub room_id: String,
	/// Statement-store topics. A new statement on any of them wakes the
	/// worker.
	pub topics: Vec<crate::Bytes32>,
}

/// Request to replace all wake watches of the product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostWorkerSetWakeWatchesRequest {
	/// The new watches. An empty list removes every watch.
	pub watches: Vec<WorkerWakeWatch>,
}

/// How a new statement on a watched topic reaches the worker.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum WorkerWakeDelivery {
	/// The host passed the topics to its push relay. A new statement wakes the
	/// worker also while the app is in the background.
	Push,
	/// The host keeps the watches, but no push relay serves them now, so
	/// nothing wakes a stopped worker.
	Unavailable {
		/// Human-readable reason.
		reason: String,
	},
}

/// Response to a wake watch replacement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostWorkerSetWakeWatchesResponse {
	/// How the watches reach the worker.
	pub delivery: WorkerWakeDelivery,
}

/// Wake watch replacement error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
pub enum HostWorkerSetWakeWatchesError {
	/// The request is over a host limit: watches, topics of one watch, or
	/// topics of all watches.
	#[display("too many wake watches or topics: {reason}")]
	TooMany {
		/// The limit the request is over.
		reason: String,
	},
	/// A watch id or room id is not a valid identifier, or two watches have
	/// the same id.
	#[display("invalid wake watch: {reason}")]
	InvalidWatch {
		/// What is wrong with the watch.
		reason: String,
	},
	/// Catch-all host failure.
	#[display("{reason}")]
	Unknown {
		/// Human-readable failure reason.
		reason: String,
	},
}
