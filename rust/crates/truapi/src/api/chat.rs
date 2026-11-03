//! Unified [`Chat`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::chat::{
		HostChatActionSubscribeError, HostChatActionSubscribeItem, HostChatActionSubscribeRequest,
		HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
		HostChatDeleteRoomError, HostChatDeleteRoomRequest, HostChatDeleteRoomResponse,
		HostChatListSubscribeError, HostChatListSubscribeItem, HostChatListSubscribeRequest,
		HostChatPostMessageError, HostChatPostMessageRequest, HostChatPostMessageResponse,
		HostChatRegisterBotError, HostChatRegisterBotRequest, HostChatRegisterBotResponse,
		HostChatSetRoomFooterError, HostChatSetRoomFooterRequest, HostChatSetRoomFooterResponse,
		HostChatSetRoomMetadataError, HostChatSetRoomMetadataRequest,
		HostChatSetRoomMetadataResponse, HostChatSetRoomPreviewError,
		HostChatSetRoomPreviewRequest, HostChatSetRoomPreviewResponse, HostChatUpdateFeedError,
		HostChatUpdateFeedRequest, HostChatUpdateFeedResponse, ProductChatMessagesQueryRequest,
		ProductChatMessagesQueryResponse, ProductChatQueryError, ProductChatReactionsQueryRequest,
		ProductChatReactionsQueryResponse, ProductChatUserActionError,
		ProductChatUserActionRequest, ProductChatUserActionResponse,
	},
	wire, wire_trait,
};

/// Chat room, bot, and message APIs.
#[wire_trait(id = 4)]
#[crate::service(required_execution = Worker)]
#[crate::async_trait]
pub trait Chat: Send + Sync {
	/// Create a chat room. The host derives the room id from the product and
	/// `roomIdSource`, so the same source gives the same room again. Every
	/// later call names the room with the `roomId` of the answer.
	///
	/// `config` declares the parts of the room that the product serves: its
	/// messages (`productSource`), who reacted (`reactionDetails`) and the
	/// reports of what the user saw (`reportSeen`). The host keeps it across
	/// restarts. Each call replaces it, also for a room that exists, so a
	/// product declares its rooms again when it starts.
	///
	/// ```ts
	/// const result = await truapi.chat.createRoom({
	///   roomIdSource: "test-room",
	///   name: "Test Room",
	///   icon: "",
	///   config: { productSource: false, reactionDetails: false, reportSeen: false },
	/// });
	/// assert(result.isOk(), "createRoom failed:", result);
	/// console.log("room created:", result.value.roomId);
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
	/// const created = await truapi.chat.createRoom({
	///   roomIdSource: "test-room",
	///   name: "Test Room",
	///   icon: "",
	///   config: { productSource: false, reactionDetails: false, reportSeen: false },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.postMessage({
	///   roomId: created.value.roomId,
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
	///   roomIdSource: "test-room",
	///   name: "Test Room",
	///   icon: "",
	///   config: { productSource: false, reactionDetails: false, reportSeen: false },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomFooter({
	///   roomId: created.value.roomId,
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

	/// Replace the name, description and empty-room text of a room, and its
	/// icon when the metadata names one.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomIdSource: "test-room",
	///   name: "Test Room",
	///   icon: "",
	///   config: { productSource: false, reactionDetails: false, reportSeen: false },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomMetadata({
	///   roomId: created.value.roomId,
	///   metadata: {
	///     name: "Renamed Room",
	///     description: "Now with a description",
	///     icon: "https://example.com/renamed.png",
	///     placeholder: "No messages yet",
	///   },
	/// });
	/// assert(result.isOk(), "setRoomMetadata failed:", result);
	/// ```
	#[wire(id = 8)]
	async fn set_room_metadata(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomMetadataRequest,
	) -> Result<HostChatSetRoomMetadataResponse, CallError<HostChatSetRoomMetadataError>> {
		Err(CallError::unavailable())
	}

	/// Replace how a room shows in the chat list. The preview gives the sort time,
	/// the badge, whether the room is pinned, and the newest message and reaction
	/// for the preview line, or an activity line that replaces it. In a room the
	/// host stores, the preview cannot show a message or a reaction of the user.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomIdSource: "test-room",
	///   name: "Test Room",
	///   icon: "",
	///   config: { productSource: false, reactionDetails: false, reportSeen: false },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomPreview({
	///   roomId: created.value.roomId,
	///   preview: {
	///     timestamp: BigInt(Date.now()),
	///     badge: { tag: "UnreadCount", value: { count: 3 } },
	///     lastMessage: {
	///       messageId: "m3",
	///       origin: { participantId: "contact" },
	///       timestamp: BigInt(Date.now()),
	///       body: { tag: "Text", value: { text: "Three new answers" } },
	///       status: "Delivered",
	///       request: false,
	///     },
	///     unseenReactions: 0,
	///     pinned: true,
	///     activity: { text: "Ben is typing", ongoing: true },
	///   },
	/// });
	/// assert(result.isOk(), "setRoomPreview failed:", result);
	/// ```
	#[wire(id = 9)]
	async fn set_room_preview(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomPreviewRequest,
	) -> Result<HostChatSetRoomPreviewResponse, CallError<HostChatSetRoomPreviewError>> {
		Err(CallError::unavailable())
	}

	/// Delete a room and all the host keeps for it: messages, configuration
	/// and cache. A delete of a room that does not exist succeeds.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomIdSource: "test-room",
	///   name: "Test Room",
	///   icon: "",
	///   config: { productSource: false, reactionDetails: false, reportSeen: false },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.deleteRoom({ roomId: created.value.roomId });
	/// assert(result.isOk(), "deleteRoom failed:", result);
	/// ```
	#[wire(id = 10)]
	async fn delete_room(
		&self,
		_cx: &CallContext,
		_request: HostChatDeleteRoomRequest,
	) -> Result<HostChatDeleteRoomResponse, CallError<HostChatDeleteRoomError>> {
		Err(CallError::unavailable())
	}

	/// Push one change to the messages of a room. The change is a new message, an
	/// edit, a reaction, a status, new content for a message with no edit mark,
	/// or a deletion.
	/// The host stores the change for a room it stores, and only shows it for a
	/// room the product keeps.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomIdSource: "test-room",
	///   name: "Test Room",
	///   icon: "",
	///   config: { productSource: false, reactionDetails: false, reportSeen: false },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.updateFeed({
	///   roomId: created.value.roomId,
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

	/// Answer one page of messages of a room the product keeps. The host asks
	/// only for a room whose configuration sets `productSource`.
	///
	/// ```ts
	/// import { ok } from "neverthrow";
	///
	/// truapi.chat.onMessagesQuery(({ roomId, anchor, pageSize }) => {
	///   console.log("page asked:", roomId, anchor.tag, pageSize);
	///   return ok({
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
	async fn messages_query(
		&self,
		_cx: &CallContext,
		_request: ProductChatMessagesQueryRequest,
	) -> Result<ProductChatMessagesQueryResponse, CallError<ProductChatQueryError>> {
		Err(CallError::unavailable())
	}

	/// Apply what the user did in a room the product keeps. For a new message,
	/// the answer is the stored message. Store it under the `messageId` of the
	/// action when the action has one. For each other action, the answer is an
	/// acknowledgement.
	///
	/// ```ts
	/// import { err, ok } from "neverthrow";
	///
	/// const created = await truapi.chat.createRoom({
	///   roomIdSource: "test-room",
	///   name: "Test Room",
	///   icon: "",
	///   config: { productSource: false, reactionDetails: false, reportSeen: false },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const { roomId: testRoom } = created.value;
	/// truapi.chat.onUserAction(({ roomId, action }) => {
	///   if (roomId !== testRoom) {
	///     return err({ tag: "Domain", value: { tag: "V2", value: { tag: "UnknownRoom" } } });
	///   }
	///   if (action.tag === "NewMessage") {
	///     return ok({
	///       tag: "MessageCreated",
	///       value: {
	///         messageId: action.value.messageId ?? `m-${Date.now()}`,
	///         origin: { participantId: "user" },
	///         timestamp: BigInt(Date.now()),
	///         body: action.value.body,
	///         status: "Sent",
	///         reply: action.value.replyTo ? { messageId: action.value.replyTo } : undefined,
	///         reactions: [],
	///         request: false,
	///       },
	///     });
	///   }
	///   return ok({ tag: "Acknowledged" });
	/// });
	/// ```
	#[wire(host_initiated, id = 13)]
	async fn user_action(
		&self,
		_cx: &CallContext,
		_request: ProductChatUserActionRequest,
	) -> Result<ProductChatUserActionResponse, CallError<ProductChatUserActionError>> {
		Err(CallError::unavailable())
	}

	/// Answer who reacted to one message. The host asks only for a room whose
	/// configuration sets `reactionDetails`.
	///
	/// ```ts
	/// import { ok } from "neverthrow";
	///
	/// truapi.chat.onReactionsQuery(({ messageId }) => {
	///   console.log("reactions asked:", messageId);
	///   return ok({
	///     reactions: [{ participantId: "user", emoji: "👍", timestamp: BigInt(Date.now()) }],
	///   });
	/// });
	/// ```
	#[wire(host_initiated, id = 14)]
	async fn reactions_query(
		&self,
		_cx: &CallContext,
		_request: ProductChatReactionsQueryRequest,
	) -> Result<ProductChatReactionsQueryResponse, CallError<ProductChatQueryError>> {
		Err(CallError::unavailable())
	}
}
