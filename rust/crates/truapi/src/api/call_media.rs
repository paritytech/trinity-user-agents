//! Unified [`CallMedia`] trait.

use crate::{
	CallContext, CallError,
	versioned::call_media::{
		HostCallMediaError, HostCallMediaResponse, HostCallMediaSignalRequest,
		HostCallMediaStartRequest, HostCallMediaStopRequest,
	},
	wasm_env, wire, wire_trait,
};

/// The audio and the video of the call of a chat room, run by the host.
///
/// The host runs the media of the call (WebRTC with its own relays) and shows
/// its call screen. The product carries the media signals of the call (the
/// offer, the answer and the network candidates, as opaque bytes) between
/// the hosts of the two sides through its own encrypted channel: each signal
/// of this host comes to the product as `ChatRoomEvent::CallSignal`, and the
/// product gives each signal of the other side to [`CallMedia::signal`]. The
/// media is end to end between the two devices.
#[wasm_env]
#[wire_trait(id = 29)]
#[crate::async_trait]
pub trait CallMedia: Send + Sync {
	/// Start the media of the call that the room shows as `Active`. The core
	/// allows it only shortly after the user pressed a call button or
	/// accepted a call in the room.
	///
	/// ```ts
	/// const result = await truapi.callMedia.start({
	///   roomId: "lobby",
	///   callId: "c1",
	///   video: false,
	///   offerer: true,
	/// });
	/// console.log("callMedia.start:", result);
	/// ```
	#[wire(id = 0)]
	async fn start(
		&self,
		_cx: &CallContext,
		_request: HostCallMediaStartRequest,
	) -> Result<HostCallMediaResponse, CallError<HostCallMediaError>> {
		Err(CallError::unavailable())
	}

	/// Give the host a media signal of the other side of the call.
	///
	/// ```ts
	/// const result = await truapi.callMedia.signal({
	///   roomId: "lobby",
	///   callId: "c1",
	///   signal: "0x00",
	/// });
	/// console.log("callMedia.signal:", result);
	/// ```
	#[wire(id = 1)]
	async fn signal(
		&self,
		_cx: &CallContext,
		_request: HostCallMediaSignalRequest,
	) -> Result<HostCallMediaResponse, CallError<HostCallMediaError>> {
		Err(CallError::unavailable())
	}

	/// Stop the media of the call: it ended.
	///
	/// ```ts
	/// const result = await truapi.callMedia.stop({ roomId: "lobby", callId: "c1" });
	/// console.log("callMedia.stop:", result);
	/// ```
	#[wire(id = 2)]
	async fn stop(
		&self,
		_cx: &CallContext,
		_request: HostCallMediaStopRequest,
	) -> Result<HostCallMediaResponse, CallError<HostCallMediaError>> {
		Err(CallError::unavailable())
	}
}
