//! Unified [`Chat`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::chat::{
		HostChatActionSubscribeError, HostChatActionSubscribeItem, HostChatActionSubscribeRequest,
		HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
		HostChatListSubscribeError, HostChatListSubscribeItem, HostChatListSubscribeRequest,
		HostChatPostMessageError, HostChatPostMessageRequest, HostChatPostMessageResponse,
		HostChatRegisterBotError, HostChatRegisterBotRequest, HostChatRegisterBotResponse,
	},
	wire, wire_trait,
};

/// Chat room, bot, and message APIs.
#[wire_trait(id = 4)]
#[crate::service(required_execution = Worker)]
#[crate::async_trait]
pub trait Chat: Send + Sync {
	/// Create a chat room.
	///
	/// ```ts
	/// const result = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   name: "Test Room",
	///   icon: "",
	/// });
	/// assert(result.isOk(), "createRoom failed:", result);
	/// console.log("room created:", result.value);
	/// ```
	#[wire(id = 0)]
	async fn create_room(
		&self,
		_cx: &CallContext,
		_request: HostChatCreateRoomRequest,
	) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
		Err(CallError::unavailable())
	}

	/// Register a chat bot.
	///
	/// ```ts
	/// const result = await truapi.chat.registerBot({
	///   botId: "test-bot",
	///   name: "Test Bot",
	///   icon: "",
	/// });
	/// assert(result.isOk(), "registerBot failed:", result);
	/// console.log("bot registered:", result.value);
	/// ```
	#[wire(id = 1)]
	async fn register_bot(
		&self,
		_cx: &CallContext,
		_request: HostChatRegisterBotRequest,
	) -> Result<HostChatRegisterBotResponse, CallError<HostChatRegisterBotError>> {
		Err(CallError::unavailable())
	}

	/// Subscribe to the list of chat rooms.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const item = await firstValueFrom(
	///   from(truapi.chat.listSubscribe()),
	/// );
	/// console.log("room list received:", item);
	/// ```
	#[wire(id = 2)]
	async fn list_subscribe(
		&self,
		_cx: &CallContext,
		_request: HostChatListSubscribeRequest,
	) -> Subscription<HostChatListSubscribeItem, CallError<HostChatListSubscribeError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Post a message to a chat room.
	///
	/// The host bounds and screens what it forwards. Message text is capped at
	/// 16 KiB and keeps line breaks and tabs, but is rejected for other
	/// control characters and for bidirectional overrides. Identifiers and
	/// display names are normalized and screened. A message carries at most 32
	/// actions and 32 media items, a custom payload at most 256 KiB, and a URL
	/// at most 2 KiB which must be `https` or an inline raster image. A
	/// rejection reports `MessageTooLarge` when the body or custom payload is
	/// over budget, and `Unknown` with a reason naming the field otherwise.
	///
	/// The returned `messageId` is the correlation key for any action the
	/// message carries: a later `actionSubscribe` trigger names it.
	///
	/// ```ts
	/// const result = await truapi.chat.postMessage({
	///   roomId: "test-room",
	///   payload: { tag: "Text", value: { text: "Hello from playground!" } },
	/// });
	/// assert(result.isOk(), "postMessage failed:", result);
	/// console.log("message posted:", result.value);
	/// ```
	#[wire(id = 3)]
	async fn post_message(
		&self,
		_cx: &CallContext,
		_request: HostChatPostMessageRequest,
	) -> Result<HostChatPostMessageResponse, CallError<HostChatPostMessageError>> {
		Err(CallError::unavailable())
	}

	/// Subscribe to received chat actions.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const item = await firstValueFrom(
	///   from(truapi.chat.actionSubscribe()),
	/// );
	/// console.log("action received:", item);
	/// ```
	#[wire(id = 4)]
	async fn action_subscribe(
		&self,
		_cx: &CallContext,
		_request: HostChatActionSubscribeRequest,
	) -> Subscription<HostChatActionSubscribeItem, CallError<HostChatActionSubscribeError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	// Id 5 is spent and must never be reassigned; the next method takes 6.
}
