use parity_scale_codec::{Decode, Encode};

/// Request to start the media of the call of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCallMediaStartRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The call that the room shows as `Active`.
	pub call_id: String,
	/// The call has video.
	pub video: bool,
	/// This side makes the offer of the media. The other side waits for it.
	pub offerer: bool,
}

/// Request to give the host a media signal of the other side of a call.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCallMediaSignalRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The call.
	pub call_id: String,
	/// The bytes of a `ChatRoomEvent::CallSignal` that the host of the other
	/// side gave to its product.
	pub signal: Vec<u8>,
}

/// Request to stop the media of the call of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCallMediaStopRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The call.
	pub call_id: String,
}

/// The host took the request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCallMediaResponse {}

/// Error of a call media request.
///
/// A host that cannot run call media answers `Unsupported` at the framework
/// level rather than through this enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostCallMediaError {
	/// The user did not press a call button or accept a call in the room a
	/// short time ago, so no microphone or camera starts.
	NotAllowed,
	/// A field breaks a limit.
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
