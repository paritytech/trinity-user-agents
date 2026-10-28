//! Unified [`Chat`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::chat::{
		HostChatActionSubscribeError, HostChatActionSubscribeItem, HostChatActionSubscribeRequest,
		HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
		HostChatDeleteRoomRequest, HostChatDeleteRoomResponse, HostChatListSubscribeError,
		HostChatListSubscribeItem, HostChatListSubscribeRequest, HostChatPostMessageError,
		HostChatPostMessageRequest, HostChatPostMessageResponse, HostChatRegisterBotError,
		HostChatRegisterBotRequest, HostChatRegisterBotResponse, HostChatRoomError,
		HostChatSetRoomConfigRequest, HostChatSetRoomConfigResponse, HostChatSetRoomFooterError,
		HostChatSetRoomFooterRequest, HostChatSetRoomFooterResponse,
		HostChatSetRoomMetadataRequest, HostChatSetRoomMetadataResponse,
		HostChatSetRoomPreviewRequest, HostChatSetRoomPreviewResponse, HostChatUpdateFeedError,
		HostChatUpdateFeedRequest, HostChatUpdateFeedResponse, ProductChatMessagesQueryItem,
		ProductChatMessagesQueryRequest, ProductChatQueryError, ProductChatReactionsQueryItem,
		ProductChatReactionsQueryRequest, ProductChatUserActionError, ProductChatUserActionItem,
		ProductChatUserActionRequest,
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

	// Id 5 is spent and must never be reassigned.

	/// Set what a room the product created shows below its messages. A room
	/// keeps the footer until the product sets another, and a new room shows
	/// the text input.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   name: "Test Room",
	///   icon: "",
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomFooter({
	///   roomId: "test-room",
	///   footer: "Empty",
	/// });
	/// assert(result.isOk(), "setRoomFooter failed:", result);
	/// ```
	#[wire(id = 6)]
	async fn set_room_footer(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomFooterRequest,
	) -> Result<HostChatSetRoomFooterResponse, CallError<HostChatSetRoomFooterError>> {
		Err(CallError::unavailable())
	}

	/// Declare the parts of a room the product serves itself: its messages
	/// (`productSource`), who reacted (`reactionDetails`) and the reports of
	/// what the user saw (`reportSeen`). The host keeps the configuration
	/// across restarts, and a new one replaces the previous one completely. A
	/// room the product declared no configuration for keeps the behaviour of a
	/// room the host stores. Declare it right after `createRoom`, before the
	/// room shows messages.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   name: "Test Room",
	///   icon: "",
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomConfig({
	///   roomId: "test-room",
	///   config: { productSource: false, reactionDetails: true, reportSeen: true },
	/// });
	/// assert(result.isOk(), "setRoomConfig failed:", result);
	/// ```
	#[wire(id = 7)]
	async fn set_room_config(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomConfigRequest,
	) -> Result<HostChatSetRoomConfigResponse, CallError<HostChatRoomError>> {
		Err(CallError::unavailable())
	}

	/// Replace the name and description of a room.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   name: "Test Room",
	///   icon: "",
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomMetadata({
	///   roomId: "test-room",
	///   metadata: { name: "Renamed Room", description: "Now with a description" },
	/// });
	/// assert(result.isOk(), "setRoomMetadata failed:", result);
	/// ```
	#[wire(id = 8)]
	async fn set_room_metadata(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomMetadataRequest,
	) -> Result<HostChatSetRoomMetadataResponse, CallError<HostChatRoomError>> {
		Err(CallError::unavailable())
	}

	/// Replace how a room shows in the chat list. The preview gives the sort time,
	/// the badge, and the newest message and reaction for the preview line.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   name: "Test Room",
	///   icon: "",
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomPreview({
	///   roomId: "test-room",
	///   preview: {
	///     timestamp: BigInt(Date.now()),
	///     badge: { tag: "UnreadCount", value: { count: 3 } },
	///     lastMessage: {
	///       messageId: "m3",
	///       origin: { participantId: "contact" },
	///       timestamp: BigInt(Date.now()),
	///       body: { tag: "Text", value: { text: "Three new answers" } },
	///       status: "Delivered",
	///       reactions: [],
	///       request: false,
	///     },
	///     unseenReactions: 0,
	///   },
	/// });
	/// assert(result.isOk(), "setRoomPreview failed:", result);
	/// ```
	#[wire(id = 9)]
	async fn set_room_preview(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomPreviewRequest,
	) -> Result<HostChatSetRoomPreviewResponse, CallError<HostChatRoomError>> {
		Err(CallError::unavailable())
	}

	/// Delete a room and all the host keeps for it: messages, configuration
	/// and cache. A delete of a room that does not exist succeeds.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   name: "Test Room",
	///   icon: "",
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.deleteRoom({ roomId: "test-room" });
	/// assert(result.isOk(), "deleteRoom failed:", result);
	/// ```
	#[wire(id = 10)]
	async fn delete_room(
		&self,
		_cx: &CallContext,
		_request: HostChatDeleteRoomRequest,
	) -> Result<HostChatDeleteRoomResponse, CallError<HostChatRoomError>> {
		Err(CallError::unavailable())
	}

	/// Push one change to the messages of a room. The change is a new message, an
	/// edit, a reaction, a status, or new content for a message with no edit mark.
	/// The host stores the change for a room it stores, and only shows it for a
	/// room the product keeps.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   name: "Test Room",
	///   icon: "",
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.updateFeed({
	///   roomId: "test-room",
	///   update: {
	///     tag: "New",
	///     value: {
	///       messageId: "welcome",
	///       origin: { participantId: "product" },
	///       timestamp: BigInt(Date.now()),
	///       body: { tag: "Text", value: { text: "Welcome!" } },
	///       status: "Sent",
	///       reactions: [],
	///       request: false,
	///     },
	///   },
	/// });
	/// assert(result.isOk(), "updateFeed failed:", result);
	/// ```
	#[wire(id = 11)]
	async fn update_feed(
		&self,
		_cx: &CallContext,
		_request: HostChatUpdateFeedRequest,
	) -> Result<HostChatUpdateFeedResponse, CallError<HostChatUpdateFeedError>> {
		Err(CallError::unavailable())
	}

	/// Answer one page of messages of a room the product keeps. Send one item,
	/// then the host closes the query. The host asks only for a room whose
	/// configuration sets `productSource`.
	///
	/// ```ts
	/// truapi.chat.onMessagesQuery(({ roomId, anchor, pageSize }, send) => {
	///   console.log("page asked:", roomId, anchor.tag, pageSize);
	///   send({
	///     messages: [
	///       {
	///         messageId: "first",
	///         origin: { participantId: "product" },
	///         timestamp: BigInt(Date.now()),
	///         body: { tag: "Text", value: { text: "The oldest message" } },
	///         status: "Sent",
	///         reactions: [],
	///         request: false,
	///       },
	///     ],
	///   });
	/// });
	/// ```
	#[wire(host_initiated, id = 12)]
	fn messages_query(
		&self,
		_cx: &CallContext,
		_request: ProductChatMessagesQueryRequest,
	) -> Subscription<ProductChatMessagesQueryItem, CallError<ProductChatQueryError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Apply what the user did in a room the product keeps. Send one item, then
	/// the host closes the query. For a new message or a payment, the item is the
	/// stored message. Store it under the `messageId` of the action when the
	/// action has one. For each other action, the item is an acknowledgement.
	///
	/// ```ts
	/// truapi.chat.onUserAction(({ roomId, action }, send, interrupt) => {
	///   if (roomId !== "test-room") {
	///     interrupt({ tag: "Domain", value: { tag: "V1", value: { tag: "UnknownRoom" } } });
	///     return;
	///   }
	///   if (action.tag === "NewMessage") {
	///     send({
	///       tag: "MessageCreated",
	///       value: {
	///         messageId: action.value.messageId ?? `m-${Date.now()}`,
	///         origin: { participantId: "user" },
	///         timestamp: BigInt(Date.now()),
	///         body: action.value.body,
	///         status: "Sent",
	///         replyTo: action.value.replyTo,
	///         reactions: [],
	///         request: false,
	///       },
	///     });
	///     return;
	///   }
	///   if (action.tag === "SendPayment") {
	///     // Send the coins to the contact before you send the answer.
	///     send({
	///       tag: "MessageCreated",
	///       value: {
	///         messageId: action.value.messageId,
	///         origin: { participantId: "user" },
	///         timestamp: action.value.timestamp,
	///         body: { tag: "Payment", value: { amount: action.value.totalValue, legacy: false } },
	///         status: "Sent",
	///         reactions: [],
	///         request: false,
	///       },
	///     });
	///     return;
	///   }
	///   send({ tag: "Acknowledged" });
	/// });
	/// ```
	#[wire(host_initiated, id = 13)]
	fn user_action(
		&self,
		_cx: &CallContext,
		_request: ProductChatUserActionRequest,
	) -> Subscription<ProductChatUserActionItem, CallError<ProductChatUserActionError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Answer who reacted to one message. Send one item, then the host closes
	/// the query. The host asks only for a room whose configuration sets
	/// `reactionDetails`.
	///
	/// ```ts
	/// truapi.chat.onReactionsQuery(({ messageId }, send) => {
	///   console.log("reactions asked:", messageId);
	///   send({ reactions: [{ participantId: "user", emoji: "👍", timestamp: BigInt(Date.now()) }] });
	/// });
	/// ```
	#[wire(host_initiated, id = 14)]
	fn reactions_query(
		&self,
		_cx: &CallContext,
		_request: ProductChatReactionsQueryRequest,
	) -> Subscription<ProductChatReactionsQueryItem, CallError<ProductChatQueryError>> {
		Subscription::interrupted(CallError::unavailable())
	}
}
