//! Unified [`Push`] trait.

use crate::{
	CallContext, CallError,
	versioned::push::{
		HostPushAddressError, HostPushAddressRequest, HostPushAddressResponse, HostPushNotifyError,
		HostPushNotifyRequest, HostPushNotifyResponse,
	},
	wasm_env, wire, wire_trait,
};

/// Notifications to the devices of contacts, through the push service of the
/// host.
///
/// A product gets a push address for each of its rooms and gives it to the
/// contact of that room through its own encrypted channel. The product of the
/// contact then asks its host to notify that address. The host encrypts the
/// title and the text for the receiving device, so the push service sees
/// neither, and the receiving device shows the notification even while the
/// app is closed. A tap opens the room.
#[wasm_env]
#[wire_trait(id = 28)]
#[crate::async_trait]
pub trait Push: Send + Sync {
	/// The push address of a room of the calling product.
	///
	/// ```ts
	/// const result = await truapi.push.address({ roomId: "lobby" });
	/// assert(result.isOk(), "push.address failed:", result);
	/// console.log(result.value.address);
	/// ```
	#[wire(id = 0)]
	async fn address(
		&self,
		_cx: &CallContext,
		_request: HostPushAddressRequest,
	) -> Result<HostPushAddressResponse, CallError<HostPushAddressError>> {
		Err(CallError::unavailable())
	}

	/// Show a notification on the device of a push address that a contact
	/// gave.
	///
	/// ```ts
	/// const address = await truapi.push.address({ roomId: "lobby" });
	/// assert(address.isOk(), "push.address failed:", address);
	/// const result = await truapi.push.notify({
	///   address: address.value.address,
	///   title: "alice.42",
	///   body: "hello",
	/// });
	/// assert(result.isOk(), "push.notify failed:", result);
	/// ```
	#[wire(id = 1)]
	async fn notify(
		&self,
		_cx: &CallContext,
		_request: HostPushNotifyRequest,
	) -> Result<HostPushNotifyResponse, CallError<HostPushNotifyError>> {
		Err(CallError::unavailable())
	}
}
