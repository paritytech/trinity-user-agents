//! Unified [`Chat`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::chat::{
		HostChatActionSubscribeError, HostChatActionSubscribeItem, HostChatActionSubscribeRequest,
		HostChatCapabilitiesError, HostChatCapabilitiesRequest, HostChatCapabilitiesResponse,
		HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
		HostChatDeleteRoomRequest, HostChatDeleteRoomResponse, HostChatListSubscribeError,
		HostChatListSubscribeItem, HostChatListSubscribeRequest, HostChatPostMessageError,
		HostChatPostMessageRequest, HostChatPostMessageResponse, HostChatRegisterBotError,
		HostChatRegisterBotRequest, HostChatRegisterBotResponse, HostChatRoomError,
		HostChatRoomEventsSubscribeError, HostChatRoomEventsSubscribeItem,
		HostChatRoomEventsSubscribeRequest, HostChatSetMessageMetaRequest,
		HostChatSetMessageMetaResponse, HostChatSetRoomActivityRequest,
		HostChatSetRoomActivityResponse, HostChatSetRoomCallRequest, HostChatSetRoomCallResponse,
		HostChatSetRoomCommandsRequest, HostChatSetRoomCommandsResponse,
		HostChatSetRoomConfigRequest, HostChatSetRoomConfigResponse,
		HostChatSetRoomFeaturesRequest, HostChatSetRoomFeaturesResponse,
		HostChatSetRoomFooterError, HostChatSetRoomFooterRequest, HostChatSetRoomFooterResponse,
		HostChatSetRoomMetadataRequest, HostChatSetRoomMetadataResponse,
		HostChatSetRoomPreviewRequest, HostChatSetRoomPreviewResponse,
		HostChatSetRoomSuggestionsRequest, HostChatSetRoomSuggestionsResponse,
		HostChatSetRoomToolbarRequest, HostChatSetRoomToolbarResponse, HostChatUpdateFeedError,
		HostChatUpdateFeedRequest, HostChatUpdateFeedResponse, ProductChatMessageActionsQueryItem,
		ProductChatMessageActionsQueryRequest, ProductChatMessagesQueryItem,
		ProductChatMessagesQueryRequest, ProductChatParticipantActionsQueryItem,
		ProductChatParticipantActionsQueryRequest, ProductChatParticipantsQueryItem,
		ProductChatParticipantsQueryRequest, ProductChatQueryError, ProductChatReactionsQueryItem,
		ProductChatReactionsQueryRequest, ProductChatSearchQueryItem,
		ProductChatSearchQueryRequest, ProductChatUserActionError, ProductChatUserActionItem,
		ProductChatUserActionRequest,
	},
	wire, wire_trait,
};

/// Chat room, bot, and message APIs.
#[wire_trait(id = 4)]
#[crate::service(required_execution = Worker)]
#[crate::async_trait]
pub trait Chat: Send + Sync {
	/// Create a chat room, or find it again. The request carries the name,
	/// avatar and description of the room, the parts of the room the product
	/// serves itself, and optionally its chat list preview. A new room that
	/// declares nothing is a room the host stores, with the host defaults.
	///
	/// ```ts
	/// const result = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room", description: "A room for tests" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
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

	/// Subscribe to what users do in the rooms of the product: posted
	/// messages, button presses, commands, edits, deletes, reactions, menu and
	/// toolbar selections, quick replies, and the room screen as it opens and
	/// closes. A posted message carries the id the host gave it.
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

	/// Set what a room the product created shows below its messages: the
	/// input with its attachment buttons, nothing, or a footer the product
	/// draws through `Renderer::render` with a `ChatFooter` context. A room
	/// keeps the footer until the product sets another, and a new room shows
	/// the input with photo and video on.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomFooter({
	///   roomId: "test-room",
	///   footer: {
	///     tag: "Input",
	///     value: { placeholder: "Ask anything", photo: true, video: false, file: true },
	///   },
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

	/// Declare the parts of a room the product serves itself. The host keeps
	/// the configuration across restarts, and a new one replaces the previous
	/// one completely. The host asks a query only when the configuration
	/// declares it.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomConfig({
	///   roomId: "test-room",
	///   config: {
	///     productSource: false,
	///     reactionDetails: true,
	///     participants: false,
	///     messageActions: true,
	///     participantActions: false,
	///     allowedReactions: ["👍", "🎉"],
	///     placeholder: "No messages yet",
	///     reportSeen: false,
	///   },
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

	/// Replace the name, avatar and description of a room.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
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

	/// Replace how a room shows in the chat list: its order, sort time, badge
	/// and preview line.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomPreview({
	///   roomId: "test-room",
	///   preview: {
	///     order: "PinToTop",
	///     timestamp: BigInt(Date.now()),
	///     badge: { tag: "UnreadCount", value: { count: 3 } },
	///     text: { text: "Three new answers", ongoing: false },
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
	/// and cache. Deleting a room that is gone succeeds.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
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

	/// Push one change to the messages of a room: a new message, an edit, a
	/// delete, a reaction, or a status. The host stores the change for a room
	/// it stores, and only shows it for a room the product keeps.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
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

	/// Replace the buttons of the room toolbar. An empty list removes the
	/// toolbar.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomToolbar({
	///   roomId: "test-room",
	///   actions: [
	///     {
	///       actionId: "settings",
	///       label: "Settings",
	///       icon: { tag: "Preimage", value: { key: "0x00" } },
	///       handler: { tag: "Callback" },
	///     },
	///   ],
	/// });
	/// assert(result.isOk(), "setRoomToolbar failed:", result);
	/// ```
	#[wire(id = 12)]
	async fn set_room_toolbar(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomToolbarRequest,
	) -> Result<HostChatSetRoomToolbarResponse, CallError<HostChatRoomError>> {
		Err(CallError::unavailable())
	}

	/// Replace the quick replies above the input. An empty list removes
	/// them.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomSuggestions({
	///   roomId: "test-room",
	///   suggestions: [
	///     { suggestionId: "yes", text: "Yes", sendsText: true },
	///     { suggestionId: "later", text: "Remind me later", sendsText: false },
	///   ],
	/// });
	/// assert(result.isOk(), "setRoomSuggestions failed:", result);
	/// ```
	#[wire(id = 13)]
	async fn set_room_suggestions(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomSuggestionsRequest,
	) -> Result<HostChatSetRoomSuggestionsResponse, CallError<HostChatRoomError>> {
		Err(CallError::unavailable())
	}

	/// Replace the slash commands the input completes. An empty list removes
	/// the catalogue.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomCommands({
	///   roomId: "test-room",
	///   commands: [
	///     { name: "help", description: "Show what the bot can do" },
	///     { name: "tip", description: "Send a tip", argumentHint: "<amount>" },
	///   ],
	/// });
	/// assert(result.isOk(), "setRoomCommands failed:", result);
	/// ```
	#[wire(id = 14)]
	async fn set_room_commands(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomCommandsRequest,
	) -> Result<HostChatSetRoomCommandsResponse, CallError<HostChatRoomError>> {
		Err(CallError::unavailable())
	}

	/// Show or remove the activity line of a room, for example "Bot is
	/// typing". The host removes it when the room screen closes.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomActivity({
	///   roomId: "test-room",
	///   activity: { text: "Bot is typing", participantId: "product" },
	/// });
	/// assert(result.isOk(), "setRoomActivity failed:", result);
	/// ```
	#[wire(id = 15)]
	async fn set_room_activity(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomActivityRequest,
	) -> Result<HostChatSetRoomActivityResponse, CallError<HostChatRoomError>> {
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
	///       },
	///     ],
	///   });
	/// });
	/// ```
	#[wire(host_initiated, id = 16)]
	fn messages_query(
		&self,
		_cx: &CallContext,
		_request: ProductChatMessagesQueryRequest,
	) -> Subscription<ProductChatMessagesQueryItem, CallError<ProductChatQueryError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Apply what the user did in a room the product keeps. Send one item,
	/// then the host closes the query: the stored message for a new message,
	/// or an acknowledgement.
	///
	/// ```ts
	/// truapi.chat.onUserAction(({ roomId, action }, send, interrupt) => {
	///   if (roomId !== "test-room") {
	///     interrupt({ tag: "Domain", value: { tag: "V1", value: { tag: "UnknownRoom" } } });
	///     return;
	///   }
	///   if (action.tag !== "NewMessage") {
	///     send({ tag: "Acknowledged" });
	///     return;
	///   }
	///   send({
	///     tag: "MessageCreated",
	///     value: {
	///       messageId: `m-${Date.now()}`,
	///       origin: { participantId: "user" },
	///       timestamp: BigInt(Date.now()),
	///       body: action.value.body,
	///       status: "Sent",
	///       replyTo: action.value.replyTo,
	///       reactions: [],
	///     },
	///   });
	/// });
	/// ```
	#[wire(host_initiated, id = 17)]
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
	///   send({ reactions: [{ participantId: "user", emoji: "👍" }] });
	/// });
	/// ```
	#[wire(host_initiated, id = 18)]
	fn reactions_query(
		&self,
		_cx: &CallContext,
		_request: ProductChatReactionsQueryRequest,
	) -> Subscription<ProductChatReactionsQueryItem, CallError<ProductChatQueryError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Answer a question about the participants of a room: their number, one
	/// page, or some of them by id. Send one item, then the host closes the
	/// query. The host asks only for a room whose configuration sets
	/// `participants`.
	///
	/// ```ts
	/// truapi.chat.onParticipantsQuery(({ query }, send) => {
	///   if (query.tag === "Count") {
	///     send({ tag: "Count", value: { count: 1 } });
	///     return;
	///   }
	///   const guide = {
	///     participantId: "guide",
	///     participant: { tag: "Custom", value: { nickname: "Guide" } },
	///   } as const;
	///   const participants = [guide];
	///   send(
	///     query.tag === "Page"
	///       ? { tag: "Page", value: { participants } }
	///       : { tag: "Resolved", value: { participants } },
	///   );
	/// });
	/// ```
	#[wire(host_initiated, id = 19)]
	fn participants_query(
		&self,
		_cx: &CallContext,
		_request: ProductChatParticipantsQueryRequest,
	) -> Subscription<ProductChatParticipantsQueryItem, CallError<ProductChatQueryError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Answer the menu for one message. Send one item, then the host closes the
	/// query. The message carries a `Custom` body with an empty payload. The
	/// host asks only for a room whose configuration sets `messageActions`.
	///
	/// ```ts
	/// truapi.chat.onMessageActionsQuery(({ message }, send) => {
	///   console.log("menu asked:", message.messageId);
	///   send({
	///     menu: {
	///       tag: "ExtendBuiltIn",
	///       value: {
	///         actions: [
	///           { actionId: "pin", label: "Pin", destructive: false, handler: { tag: "Callback" } },
	///         ],
	///         hidden: ["Edit"],
	///       },
	///     },
	///   });
	/// });
	/// ```
	#[wire(host_initiated, id = 20)]
	fn message_actions_query(
		&self,
		_cx: &CallContext,
		_request: ProductChatMessageActionsQueryRequest,
	) -> Subscription<ProductChatMessageActionsQueryItem, CallError<ProductChatQueryError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Answer the menu for one participant. Send one item, then the host closes
	/// the query. The host asks only for a room whose configuration sets
	/// `participantActions`.
	///
	/// ```ts
	/// truapi.chat.onParticipantActionsQuery(({ entry }, send) => {
	///   console.log("menu asked:", entry.participantId);
	///   send({
	///     menu: {
	///       tag: "Custom",
	///       value: {
	///         actions: [
	///           { actionId: "mute", label: "Mute", destructive: true, handler: { tag: "Callback" } },
	///         ],
	///       },
	///     },
	///   });
	/// });
	/// ```
	#[wire(host_initiated, id = 21)]
	fn participant_actions_query(
		&self,
		_cx: &CallContext,
		_request: ProductChatParticipantActionsQueryRequest,
	) -> Subscription<ProductChatParticipantActionsQueryItem, CallError<ProductChatQueryError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// The Chat features this host serves, by name, and its attachment
	/// bounds. A host lists a feature only when it serves it from end to end.
	/// A host that knows no such request answers `unavailable`; the product
	/// then keeps to the features of every v2 host.
	///
	/// ```ts
	/// const result = await truapi.chat.capabilities();
	/// assert(result.isOk(), "capabilities failed:", result);
	/// console.log("features:", result.value.features.join(", "));
	/// ```
	#[wire(id = 22)]
	async fn capabilities(
		&self,
		_cx: &CallContext,
		_request: HostChatCapabilitiesRequest,
	) -> Result<HostChatCapabilitiesResponse, CallError<HostChatCapabilitiesError>> {
		Err(CallError::unavailable())
	}

	/// Declare the optional features of a room. The host keeps the
	/// declaration across restarts, and a new one replaces the previous one
	/// completely. The host keeps a feature it does not serve and ignores it.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomFeatures({
	///   roomId: "test-room",
	///   features: {
	///     preimageAttachments: false,
	///     search: false,
	///     calls: { audio: true, video: false },
	///     readAloud: true,
	///   },
	/// });
	/// assert(result.isOk(), "setRoomFeatures failed:", result);
	/// ```
	#[wire(id = 23)]
	async fn set_room_features(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomFeaturesRequest,
	) -> Result<HostChatSetRoomFeaturesResponse, CallError<HostChatRoomError>> {
		Err(CallError::unavailable())
	}

	/// Replace the thread summary, the branch position and the reasoning flag
	/// of one message. The host keeps the meta in both room modes. It counts
	/// against the `updateFeed` budget of the room.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const posted = await truapi.chat.updateFeed({
	///   roomId: "test-room",
	///   update: {
	///     tag: "New",
	///     value: {
	///       messageId: "answer",
	///       origin: { participantId: "product" },
	///       timestamp: BigInt(Date.now()),
	///       body: { tag: "Text", value: { text: "Here is one answer" } },
	///       status: "Sent",
	///       reactions: [],
	///     },
	///   },
	/// });
	/// assert(posted.isOk(), "updateFeed failed:", posted);
	/// const result = await truapi.chat.setMessageMeta({
	///   roomId: "test-room",
	///   messageId: "answer",
	///   meta: { branch: { index: 1, count: 2 } },
	/// });
	/// assert(result.isOk(), "setMessageMeta failed:", result);
	/// ```
	#[wire(id = 24)]
	async fn set_message_meta(
		&self,
		_cx: &CallContext,
		_request: HostChatSetMessageMetaRequest,
	) -> Result<HostChatSetMessageMetaResponse, CallError<HostChatUpdateFeedError>> {
		Err(CallError::unavailable())
	}

	/// Show, change or remove the call of a room. A call that rings or a call
	/// without a media session needs the `call_buttons` feature of the host.
	///
	/// ```ts
	/// const created = await truapi.chat.createRoom({
	///   roomId: "test-room",
	///   metadata: { name: "Test Room" },
	///   config: {
	///     productSource: false,
	///     reactionDetails: false,
	///     participants: false,
	///     messageActions: false,
	///     participantActions: false,
	///     reportSeen: false,
	///   },
	/// });
	/// assert(created.isOk(), "createRoom failed:", created);
	/// const result = await truapi.chat.setRoomCall({
	///   roomId: "test-room",
	///   call: {
	///     tag: "Active",
	///     value: { callId: "call-1", video: false, startedAt: BigInt(Date.now()) },
	///   },
	/// });
	/// console.log("setRoomCall:", result);
	/// ```
	#[wire(id = 25)]
	async fn set_room_call(
		&self,
		_cx: &CallContext,
		_request: HostChatSetRoomCallRequest,
	) -> Result<HostChatSetRoomCallResponse, CallError<HostChatRoomError>> {
		Err(CallError::unavailable())
	}

	/// Subscribe to what the user does with the optional features of the
	/// rooms of the product: mute and archive, wakes, calls and branches. The
	/// stream starts with the state of each room the user muted or archived.
	/// A second subscription replaces the first.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const item = await firstValueFrom(
	///   from(truapi.chat.roomEventsSubscribe()),
	/// );
	/// console.log("room event received:", item);
	/// ```
	#[wire(id = 26)]
	async fn room_events_subscribe(
		&self,
		_cx: &CallContext,
		_request: HostChatRoomEventsSubscribeRequest,
	) -> Subscription<HostChatRoomEventsSubscribeItem, CallError<HostChatRoomEventsSubscribeError>>
	{
		Subscription::interrupted(CallError::unavailable())
	}

	/// Answer the messages of a room the product keeps that match what the
	/// user typed, newest first. Send one item, then the host closes the
	/// query. The host asks only for a room whose configuration sets
	/// `productSource` and whose features set `search`.
	///
	/// ```ts
	/// truapi.chat.onSearchQuery(({ roomId, text, pageSize }, send) => {
	///   console.log("search asked:", roomId, text, pageSize);
	///   send({ hits: [] });
	/// });
	/// ```
	#[wire(host_initiated, id = 27)]
	fn search_query(
		&self,
		_cx: &CallContext,
		_request: ProductChatSearchQueryRequest,
	) -> Subscription<ProductChatSearchQueryItem, CallError<ProductChatQueryError>> {
		Subscription::interrupted(CallError::unavailable())
	}
}
