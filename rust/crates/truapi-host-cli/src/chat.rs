//! In-memory Chat host for the CLI.
//!
//! Rooms, bots, declarations and messages live for the length of the process:
//! this exists to make a chat product runnable headlessly, not to be a chat
//! backend.
//!
//! Every message and declaration the core hands over is appended to the
//! transcript named by `TRUAPI_CHAT_LOG`, one JSON object per line. A
//! product-visible error alone cannot tell "the core rejected this before any
//! host saw it" apart from "the host was handed it and refused", and that
//! distinction is the whole point of screening content in the runtime; the
//! transcript is what lets a battery assert the first reading.
//!
//! The same state is what the development chat surface draws. A room keeps
//! what the product declared about it (metadata, configuration, preview and
//! footer), and its messages: every message of a room the host keeps, and a
//! page cache of a room the product keeps. Each change goes to every observer
//! as the JSON the surface protocol names.

use std::{
	collections::{BTreeMap, BTreeSet},
	path::PathBuf,
	sync::{Arc, Mutex},
	time::{SystemTime, UNIX_EPOCH},
};

use futures::{
	StreamExt,
	channel::mpsc,
	stream::{self, BoxStream},
};
use parity_scale_codec::Encode;
use serde_json::{Value, json};
use truapi::{
	latest::{
		ChatBotRegistrationStatus, ChatCallRecord, ChatContactEvent, ChatFeedUpdate, ChatMediaItem,
		ChatMediaMeta, ChatMessage, ChatMessageBody, ChatMessageContent, ChatMessageOrigin,
		ChatMessageStatus, ChatReactionSummary, ChatResource, ChatRoom, ChatRoomBadge,
		ChatRoomConfig, ChatRoomFooter, ChatRoomMetadata, ChatRoomParticipation, ChatRoomPreview,
		ChatRoomRegistrationStatus, ChatUserAction, GenericError, HostChatCreateRoomError,
		HostChatCreateRoomRequest, HostChatCreateRoomResponse, HostChatDeleteRoomRequest,
		HostChatListSubscribeItem, HostChatPostMessageError, HostChatPostMessageRequest,
		HostChatPostMessageResponse, HostChatRegisterBotError, HostChatRegisterBotRequest,
		HostChatRegisterBotResponse, HostChatSetRoomConfigRequest, HostChatSetRoomFooterRequest,
		HostChatSetRoomMetadataRequest, HostChatSetRoomPreviewRequest, HostChatUpdateFeedError,
		HostChatUpdateFeedRequest, ProductChatUserActionItem,
	},
	platform::{ChatPlatform, ProductContext, async_trait},
};

use crate::transcript::Transcript;

/// The reserved participant id of the host's user.
pub const USER: &str = "user";

/// The list text of a message that has no text of its own.
const SENT_A_MESSAGE: &str = "Sent a message";

/// A room this host created, with everything the product declared about it.
struct RoomRecord {
	participating_as: ChatRoomParticipation,
	metadata: ChatRoomMetadata,
	/// The icon the room was created with, or empty.
	icon_url: String,
	config: ChatRoomConfig,
	preview: Option<ChatRoomPreview>,
	footer: ChatRoomFooter,
	/// In a room the host keeps, every message in the order it arrived. In a
	/// room the product keeps, the messages the screen saw: the page cache.
	messages: Vec<StoredMessage>,
	/// The newest message the user saw.
	last_seen: Option<String>,
}

impl RoomRecord {
	/// A room that declared nothing yet: the host keeps its messages, and it
	/// shows the text input.
	fn new(participating_as: ChatRoomParticipation, name: String, icon_url: String) -> Self {
		Self {
			participating_as,
			metadata: ChatRoomMetadata { name, description: None },
			icon_url,
			config: ChatRoomConfig::default(),
			preview: None,
			footer: ChatRoomFooter::TextInput,
			messages: Vec::new(),
			last_seen: None,
		}
	}

	fn position(&self, message_id: &str) -> Option<usize> {
		self.messages.iter().position(|stored| stored.message.message_id == message_id)
	}

	/// Whether the host keeps the messages of the room.
	fn host_store(&self) -> bool {
		!self.config.product_source
	}

	/// The message the chat list shows: the newest to arrive in a room the
	/// host keeps, the newest by time in the page cache of a room the
	/// product keeps.
	fn last_message(&self) -> Option<&ChatMessage> {
		let mut messages = self.messages.iter();
		if self.host_store() {
			messages.next_back().map(|stored| &stored.message)
		} else {
			messages
				.max_by_key(|stored| stored.message.timestamp)
				.map(|stored| &stored.message)
		}
	}

	/// Messages of others after the newest one the user saw.
	fn unread(&self) -> usize {
		let after = self
			.last_seen
			.as_deref()
			.and_then(|seen| self.position(seen))
			.map_or(0, |index| index + 1);
		self.messages[after.min(self.messages.len())..]
			.iter()
			.filter(|stored| stored.message.origin.participant_id != USER)
			.count()
	}
}

/// One message and what the host tracks beside it.
struct StoredMessage {
	message: ChatMessage,
	/// Who placed each reaction, as `(participant, emoji)`, in a room the
	/// host keeps. The summaries of a room the product keeps come from the
	/// product, so this stays empty there.
	reactions: Vec<(String, String)>,
}

impl StoredMessage {
	fn new(message: ChatMessage) -> Self {
		Self { message, reactions: Vec::new() }
	}

	/// Place or remove one reaction. `tracked` keeps the reaction book and
	/// derives the summaries from it; otherwise the summaries change in place.
	fn react(&mut self, participant_id: &str, emoji: &str, placed: bool, tracked: bool) {
		if tracked {
			let entry = (participant_id.to_string(), emoji.to_string());
			let held = self.reactions.iter().position(|reaction| *reaction == entry);
			match (held, placed) {
				(None, true) => self.reactions.push(entry),
				(Some(index), false) => {
					self.reactions.remove(index);
				},
				_ => {},
			}
			self.message.reactions = summaries(&self.reactions);
			return;
		}
		let user = participant_id == USER;
		let summaries = &mut self.message.reactions;
		match summaries.iter().position(|summary| summary.emoji == emoji) {
			Some(index) if placed => {
				summaries[index].count += 1;
				summaries[index].reacted_by_user |= user;
			},
			Some(index) => {
				let summary = &mut summaries[index];
				summary.count = summary.count.saturating_sub(1);
				if user {
					summary.reacted_by_user = false;
				}
				if summary.count == 0 {
					summaries.remove(index);
				}
			},
			None if placed => summaries.push(ChatReactionSummary {
				emoji: emoji.to_string(),
				count: 1,
				reacted_by_user: user,
				unseen: false,
			}),
			None => {},
		}
	}
}

/// Summaries of a reaction book, one per emoji in the order each first
/// appeared.
fn summaries(reactions: &[(String, String)]) -> Vec<ChatReactionSummary> {
	let mut out: Vec<ChatReactionSummary> = Vec::new();
	for (participant_id, emoji) in reactions {
		let user = participant_id == USER;
		match out.iter_mut().find(|summary| summary.emoji == *emoji) {
			Some(summary) => {
				summary.count += 1;
				summary.reacted_by_user |= user;
			},
			None => out.push(ChatReactionSummary {
				emoji: emoji.clone(),
				count: 1,
				reacted_by_user: user,
				unseen: false,
			}),
		}
	}
	out
}

/// Rooms, bots and messages for one process.
#[derive(Default)]
struct State {
	/// Room id to what the product created and declared.
	rooms: BTreeMap<String, RoomRecord>,
	/// Registered bot ids. A bot is not a room, so registering one does not
	/// republish the room list.
	bots: BTreeSet<String>,
	/// How many message ids the host gave. A product correlates an action
	/// trigger against that id, so product and user messages share one
	/// sequence.
	assigned: u64,
	/// Live room-list subscribers, one per product connection.
	subscribers: Vec<mpsc::UnboundedSender<HostChatListSubscribeItem>>,
	/// Live surface observers. Each gets the surface-protocol events.
	observers: Vec<mpsc::UnboundedSender<Value>>,
}

impl State {
	/// The next host-assigned id that `room` does not hold.
	fn next_id(&mut self, room_id: &str) -> String {
		loop {
			self.assigned += 1;
			let id = format!("m{}", self.assigned);
			let taken = self.rooms.get(room_id).is_some_and(|room| room.position(&id).is_some());
			if !taken {
				return id;
			}
		}
	}
}

/// Everything an observer is shown on subscribing, as surface-protocol JSON.
pub struct ChatSnapshot {
	/// `Room` views in room-id order.
	pub rooms: Vec<Value>,
	/// `{ roomId, message }` for every message of a room the host keeps, in
	/// the order each arrived.
	pub messages: Vec<Value>,
}

/// A chat host that keeps everything in memory.
pub struct CliChatHost {
	state: Mutex<State>,
	transcript: Transcript,
}

/// Milliseconds since the Unix epoch.
fn now_millis() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
		.unwrap_or_default()
}

fn unknown_room(room_id: &str) -> String {
	format!("unknown room {room_id:?}")
}

fn unknown_message(message_id: &str) -> String {
	format!("unknown message {message_id:?}")
}

impl CliChatHost {
	/// Build a chat host, writing a transcript when `TRUAPI_CHAT_LOG` names a
	/// path.
	pub fn from_env() -> Arc<Self> {
		Self::new(std::env::var_os("TRUAPI_CHAT_LOG").map(PathBuf::from))
	}

	/// Build a chat host recording to `transcript`.
	pub fn new(transcript: Option<PathBuf>) -> Arc<Self> {
		Arc::new(Self {
			state: Mutex::new(State::default()),
			transcript: Transcript::new(transcript, "chat"),
		})
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, State> {
		self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// Current state plus every change after it. Both are taken under one
	/// lock, so an observer neither misses nor repeats a change that lands
	/// while it subscribes.
	pub fn observe(&self) -> (ChatSnapshot, mpsc::UnboundedReceiver<Value>) {
		let mut state = self.lock();
		let snapshot = ChatSnapshot {
			rooms: state.rooms.iter().map(|(room_id, room)| room_json(room_id, room)).collect(),
			messages: state
				.rooms
				.iter()
				.filter(|(_, room)| room.host_store())
				.flat_map(|(room_id, room)| {
					room.messages.iter().map(
						move |stored| json!({ "roomId": room_id, "message": message_json(&stored.message) }),
					)
				})
				.collect(),
		};
		let (sender, receiver) = mpsc::unbounded();
		state.observers.push(sender);
		(snapshot, receiver)
	}

	/// The configuration the product declared for `room_id`, if the room
	/// exists.
	pub fn room_config(&self, room_id: &str) -> Option<ChatRoomConfig> {
		self.lock().rooms.get(room_id).map(|room| room.config.clone())
	}

	/// A message the room holds or the page cache saw.
	#[cfg(test)]
	pub fn message(&self, room_id: &str, message_id: &str) -> Option<ChatMessage> {
		let state = self.lock();
		let room = state.rooms.get(room_id)?;
		room.position(message_id).map(|index| room.messages[index].message.clone())
	}

	/// Keep one page the product answered in the page cache of a room it
	/// keeps. The page goes back to the surface that asked for it, so no
	/// observer is told.
	pub fn cache_page(&self, room_id: &str, messages: &[ChatMessage]) {
		let mut state = self.lock();
		let Some(room) = state.rooms.get_mut(room_id) else {
			return;
		};
		if room.host_store() {
			return;
		}
		for message in messages {
			match room.position(&message.message_id) {
				Some(index) => room.messages[index] = StoredMessage::new(message.clone()),
				None => room.messages.push(StoredMessage::new(message.clone())),
			}
		}
	}

	/// Store a message the user sent into a room the host keeps, and tell
	/// every observer. The host gives the id.
	pub fn post_user_message(
		&self,
		room_id: &str,
		body: ChatMessageBody,
		reply_to: Option<String>,
	) -> Result<ChatMessage, String> {
		let mut state = self.lock();
		let room = state.rooms.get(room_id).ok_or_else(|| unknown_room(room_id))?;
		if !room.host_store() {
			return Err("the room keeps its messages in the product".to_string());
		}
		let message_id = state.next_id(room_id);
		let message = ChatMessage {
			message_id,
			origin: ChatMessageOrigin { participant_id: USER.to_string() },
			timestamp: now_millis(),
			body,
			status: ChatMessageStatus::Sent,
			reply_to,
			reply_preview: None,
			edited_at: None,
			reactions: Vec::new(),
			request: false,
		};
		let line = json!({
			"kind": "message",
			"messageId": message.message_id,
			"roomId": room_id,
			"author": "person",
			"variant": body_variant(&message.body),
			"payload": hex::encode(message.body.encode()),
			"replyTo": message.reply_to,
		});
		let room = state.rooms.get_mut(room_id).expect("the room was just found");
		room.messages.push(StoredMessage::new(message.clone()));
		let index = room.messages.len() - 1;
		Self::announce_message(&mut state, room_id, index);
		drop(state);
		self.record(line);
		Ok(message)
	}

	/// Mark `message_id` as the newest message the user saw in `room_id`. The
	/// answer is `true` when the product asked for seen reports.
	pub fn mark_seen(&self, room_id: &str, message_id: &str) -> Result<bool, String> {
		let mut state = self.lock();
		let room = state.rooms.get_mut(room_id).ok_or_else(|| unknown_room(room_id))?;
		room.position(message_id).ok_or_else(|| unknown_message(message_id))?;
		room.last_seen = Some(message_id.to_string());
		let report = room.config.report_seen;
		let event = with_kind("room", room_json(room_id, room));
		Self::broadcast(&mut state, event);
		Ok(report)
	}

	/// Apply what the product answered to a user action in a room it keeps,
	/// so the screen shows the outcome.
	pub fn apply_source_action(
		&self,
		room_id: &str,
		action: &ChatUserAction,
		answer: &ProductChatUserActionItem,
	) {
		let mut state = self.lock();
		let Some(room) = state.rooms.get_mut(room_id) else {
			return;
		};
		let changed = match (action, answer) {
			(
				ChatUserAction::NewMessage { .. } | ChatUserAction::SendPayment { .. },
				ProductChatUserActionItem::MessageCreated(message),
			) => match room.position(&message.message_id) {
				Some(index) => {
					room.messages[index] = StoredMessage::new(message.clone());
					Some(index)
				},
				None => {
					room.messages.push(StoredMessage::new(message.clone()));
					Some(room.messages.len() - 1)
				},
			},
			(_, ProductChatUserActionItem::MessageCreated(_)) => None,
			(ChatUserAction::EditMessage { message_id, body }, _) => {
				room.position(message_id).inspect(|&index| {
					let message = &mut room.messages[index].message;
					message.body = body.clone();
					message.edited_at = Some(now_millis());
				})
			},
			(ChatUserAction::ReactionPlaced { message_id, emoji }, _) => room
				.position(message_id)
				.inspect(|&index| room.messages[index].react(USER, emoji, true, false)),
			(ChatUserAction::ReactionRemoved { message_id, emoji }, _) => room
				.position(message_id)
				.inspect(|&index| room.messages[index].react(USER, emoji, false, false)),
			(ChatUserAction::Seen { message_id }, _) => {
				room.last_seen = Some(message_id.clone());
				None
			},
			// A send the product only acknowledged changes no message the
			// screen holds, and the page sends no payment or contact action.
			(
				ChatUserAction::NewMessage { .. } |
				ChatUserAction::SendPayment { .. } |
				ChatUserAction::AcceptRequest |
				ChatUserAction::DeclineRequest |
				ChatUserAction::Block |
				ChatUserAction::Unblock |
				ChatUserAction::Leave,
				_,
			) => None,
		};
		if let Some(index) = changed {
			Self::announce_message(&mut state, room_id, index);
		}
	}

	/// Tell every observer one message changed, with the chat-list view of
	/// its room, which the change can move.
	fn announce_message(state: &mut State, room_id: &str, index: usize) {
		let Some(room) = state.rooms.get(room_id) else {
			return;
		};
		let event = json!({
			"kind": "message",
			"roomId": room_id,
			"persisted": room.host_store(),
			"message": message_json(&room.messages[index].message),
			"list": list_json(room),
		});
		Self::broadcast(state, event);
	}

	/// Give a first-version message the product posted its id, and append it
	/// to the transcript. Product and user messages share one id sequence.
	fn accept_legacy(&self, request: &HostChatPostMessageRequest) -> Option<String> {
		let mut state = self.lock();
		if !state.rooms.contains_key(&request.room_id) {
			return None;
		}
		let message_id = state.next_id(&request.room_id);
		drop(state);
		self.record(json!({
			"kind": "message",
			"messageId": message_id,
			"roomId": request.room_id,
			"author": "product",
			"variant": variant_name(&request.payload),
			// The payload as the host received it. A summary would let a
			// difference between what a product sent and what a host stored
			// hide behind the summary.
			"payload": hex::encode(request.payload.encode()),
			"alt": request.alt,
		}));
		Some(message_id)
	}

	/// Change one declaration of a room the product hosts, tell every
	/// observer the new room view, and append the request to the transcript.
	fn declare<R: Encode>(
		&self,
		kind: &str,
		room_id: &str,
		request: &R,
		change: impl FnOnce(&mut RoomRecord),
	) -> Result<(), GenericError> {
		let mut state = self.lock();
		let room = state
			.rooms
			.get_mut(room_id)
			.ok_or_else(|| GenericError { reason: unknown_room(room_id) })?;
		change(room);
		let event = with_kind("room", room_json(room_id, room));
		Self::broadcast(&mut state, event);
		drop(state);
		self.record_declaration(kind, room_id, request);
		Ok(())
	}

	/// Current room list, in room-id order so a replacement that changes
	/// nothing is byte-identical to the one before it.
	fn room_list(state: &State) -> HostChatListSubscribeItem {
		HostChatListSubscribeItem {
			rooms: state
				.rooms
				.iter()
				.map(|(room_id, room)| ChatRoom {
					room_id: room_id.clone(),
					participating_as: room.participating_as,
				})
				.collect(),
		}
	}

	/// Send the current list to every live subscriber, dropping closed ones.
	fn republish(state: &mut State) {
		let item = Self::room_list(state);
		state
			.subscribers
			.retain(|subscriber| subscriber.unbounded_send(item.clone()).is_ok());
	}

	/// Send one event to every live observer, dropping closed ones. Called
	/// with the state locked, so observers see changes in the order they
	/// happened.
	fn broadcast(state: &mut State, event: Value) {
		state
			.observers
			.retain(|observer| observer.unbounded_send(event.clone()).is_ok());
	}

	/// Append one accepted room or bot registration.
	fn record_registration(&self, kind: &str, id: &str, name: &str, icon: &str) {
		self.record(json!({
			"kind": kind,
			"id": id,
			"name": name,
			// Icons resolve before a host sees them, so record what arrived
			// rather than what the product typed.
			"icon": icon,
		}));
	}

	/// Append one declaration as the host received it.
	fn record_declaration<R: Encode>(&self, kind: &str, room_id: &str, request: &R) {
		self.record(json!({
			"kind": kind,
			"roomId": room_id,
			"payload": hex::encode(request.encode()),
		}));
	}

	/// Append one line to the transcript, if one is configured.
	fn record(&self, line: Value) {
		self.transcript.record(line);
	}
}

/// Apply one feed update to `room`. The answer is the index of the message
/// that changed. With `persist` the room is the host's store and the host refuses
/// what a store cannot hold; without it the update is for the screen only,
/// and an update for a message the cache never saw changes nothing.
fn apply_feed(
	room: &mut RoomRecord,
	update: &ChatFeedUpdate,
	persist: bool,
) -> Result<Option<usize>, HostChatUpdateFeedError> {
	let missing = || if persist { Err(HostChatUpdateFeedError::UnknownMessage) } else { Ok(None) };
	let index = match update {
		ChatFeedUpdate::New(message) => {
			return match room.position(&message.message_id) {
				Some(_) if persist => Err(HostChatUpdateFeedError::DuplicateMessage),
				Some(index) => {
					room.messages[index] = StoredMessage::new(message.clone());
					Ok(Some(index))
				},
				None => {
					room.messages.push(StoredMessage::new(message.clone()));
					Ok(Some(room.messages.len() - 1))
				},
			};
		},
		ChatFeedUpdate::Edited { message_id, .. } |
		ChatFeedUpdate::ReactionPlaced { message_id, .. } |
		ChatFeedUpdate::ReactionRemoved { message_id, .. } |
		ChatFeedUpdate::StatusChanged { message_id, .. } => match room.position(message_id) {
			Some(index) => index,
			None => return missing(),
		},
		ChatFeedUpdate::Replaced(message) => match room.position(&message.message_id) {
			Some(index) => index,
			None => return missing(),
		},
	};
	let stored = &mut room.messages[index];
	match update {
		ChatFeedUpdate::New(_) => unreachable!("handled above"),
		ChatFeedUpdate::Edited { body, edited_at, .. } => {
			// In the host's store the user's words are the user's own.
			if persist && stored.message.origin.participant_id == USER {
				return Err(HostChatUpdateFeedError::NotOwner);
			}
			stored.message.body = body.clone();
			stored.message.edited_at = Some(*edited_at);
		},
		// Drawn again in place; an edit mark comes only from an edit.
		ChatFeedUpdate::Replaced(message) => {
			stored.message = message.clone();
		},
		ChatFeedUpdate::ReactionPlaced { participant_id, emoji, .. } => {
			stored.react(participant_id, emoji, true, persist);
		},
		ChatFeedUpdate::ReactionRemoved { participant_id, emoji, .. } => {
			stored.react(participant_id, emoji, false, persist);
		},
		ChatFeedUpdate::StatusChanged { status, .. } => stored.message.status = *status,
	}
	Ok(Some(index))
}

/// The variant name and target message of a feed update, for the transcript.
fn feed_target(update: &ChatFeedUpdate) -> (&'static str, &str) {
	match update {
		ChatFeedUpdate::New(message) => ("New", &message.message_id),
		ChatFeedUpdate::Edited { message_id, .. } => ("Edited", message_id),
		ChatFeedUpdate::Replaced(message) => ("Replaced", &message.message_id),
		ChatFeedUpdate::ReactionPlaced { message_id, .. } => ("ReactionPlaced", message_id),
		ChatFeedUpdate::ReactionRemoved { message_id, .. } => ("ReactionRemoved", message_id),
		ChatFeedUpdate::StatusChanged { message_id, .. } => ("StatusChanged", message_id),
	}
}

/// `body` with the surface-protocol `kind` discriminant added.
fn with_kind(kind: &str, mut body: Value) -> Value {
	body["kind"] = Value::from(kind);
	body
}

/// The chat-list text of one message: its text, the `alt` of its first
/// media item, the line of an event, a call or a payment, the `alt` of a
/// custom message, or a generic line.
pub fn preview_text(message: &ChatMessage) -> String {
	let text = match &message.body {
		ChatMessageBody::Text { text } => Some(text.clone()),
		ChatMessageBody::RichText { text, media } => text
			.clone()
			.or_else(|| media.first().and_then(|item| media_meta(item).alt.clone())),
		ChatMessageBody::Event(event) => Some(event_line(*event).to_string()),
		ChatMessageBody::Call(call) => Some(call_line(call)),
		ChatMessageBody::Payment(payment) => Some(format!("Payment of {}", payment.amount)),
		ChatMessageBody::Custom { alt, .. } => alt.clone(),
	};
	text.filter(|text| !text.trim().is_empty())
		.unwrap_or_else(|| SENT_A_MESSAGE.to_string())
}

/// The line of an event about the conversation.
fn event_line(event: ChatContactEvent) -> &'static str {
	match event {
		ChatContactEvent::RequestAccepted => "Request accepted",
		ChatContactEvent::ContactAdded => "Contact added",
		ChatContactEvent::LeftChat => "Left the chat",
		ChatContactEvent::Unavailable => "Messages unavailable",
		ChatContactEvent::Unsupported => "Unsupported message",
	}
}

/// The line of a call: its kind, then how it stands.
fn call_line(call: &ChatCallRecord) -> String {
	let kind = if call.video { "Video call" } else { "Audio call" };
	let state = match (call.answered_at, call.closed) {
		(None, None) => "ringing",
		(Some(_), None) => "ongoing",
		(Some(_), Some(_)) => "ended",
		(None, Some(_)) => "not answered",
	};
	format!("{kind}, {state}")
}

fn media_meta(item: &ChatMediaItem) -> &ChatMediaMeta {
	match item {
		ChatMediaItem::Image { meta, .. } |
		ChatMediaItem::Video { meta, .. } |
		ChatMediaItem::File { meta, .. } => meta,
	}
}

/// The surface view of a room: what the product declared, and the chat-list
/// row the host derives from it.
fn room_json(room_id: &str, room: &RoomRecord) -> Value {
	json!({
		"roomId": room_id,
		"participatingAs": match room.participating_as {
			ChatRoomParticipation::RoomHost => "RoomHost",
			ChatRoomParticipation::Bot => "Bot",
		},
		"name": room.metadata.name,
		"description": room.metadata.description,
		"icon": room.icon_url,
		"config": config_json(&room.config),
		"footer": footer_json(&room.footer),
		"lastSeen": room.last_seen,
		"list": list_json(room),
	})
}

/// The chat-list row of a room: the product's preview where it set one, the
/// host default of each field where it did not. The line comes from the
/// newest message the preview names, else from the newest message the host
/// holds. A newer call the preview names apart shows beside it, and does not
/// replace the line.
fn list_json(room: &RoomRecord) -> Value {
	let preview = room.preview.as_ref();
	let last = preview
		.and_then(|preview| preview.last_message.as_ref())
		.or_else(|| room.last_message());
	let reaction = preview.and_then(|preview| preview.last_reaction.as_ref());
	let call = preview.and_then(|preview| preview.last_call.as_ref());
	let badge = match preview.and_then(|preview| preview.badge.as_ref()) {
		Some(ChatRoomBadge::UnreadCount { count }) if *count > 0 => {
			json!({ "type": "UnreadCount", "count": count })
		},
		// Matched by shape, not by name: an unread count of 0 and the
		// variant that hides the badge both show nothing.
		Some(_) => Value::Null,
		// A room the product keeps shows no badge it did not set.
		None if !room.host_store() => Value::Null,
		None => match room.unread() {
			0 => Value::Null,
			count => json!({ "type": "UnreadCount", "count": count }),
		},
	};
	json!({
		"timestamp": preview
			.and_then(|preview| preview.timestamp)
			.or_else(|| last.map(|message| message.timestamp)),
		"text": last.map(preview_text),
		"origin": last.map(|message| &message.origin.participant_id),
		"lastCall": call.map(|call| json!({
			"messageId": call.message_id,
			"text": preview_text(call),
		})),
		"lastReaction": reaction.map(|reaction| json!({
			"emoji": reaction.emoji,
			"byUser": reaction.by_user,
			"messageId": reaction.message.as_ref().map(|message| &message.message_id),
			"removed": reaction.removed,
		})),
		"unseenReactions": preview.map_or(0, |preview| preview.unseen_reactions),
		"badge": badge,
		"previewDeclared": preview.is_some(),
	})
}

fn config_json(config: &ChatRoomConfig) -> Value {
	json!({
		"productSource": config.product_source,
		"reactionDetails": config.reaction_details,
		"reportSeen": config.report_seen,
	})
}

fn footer_json(footer: &ChatRoomFooter) -> Value {
	match footer {
		ChatRoomFooter::TextInput => json!({ "type": "TextInput" }),
		ChatRoomFooter::Empty => json!({ "type": "Empty" }),
	}
}

/// The surface view of a resource. The ticket of a HOP file opens it, so the
/// page gets the node and the size only.
fn resource_json(resource: &ChatResource) -> Value {
	let ChatResource::Hop { identifier, node, size, .. } = resource;
	json!({
		"type": "Hop",
		"size": size,
		"node": node,
		"identifier": format!("0x{}", hex::encode(identifier)),
	})
}

/// The surface view of one message.
pub fn message_json(message: &ChatMessage) -> Value {
	json!({
		"messageId": message.message_id,
		"origin": message.origin.participant_id,
		"timestamp": message.timestamp,
		"body": body_json(&message.body),
		"status": status_name(message.status),
		"replyTo": message.reply_to,
		"editedAt": message.edited_at,
		"reactions": message.reactions.iter().map(|reaction| json!({
			"emoji": reaction.emoji,
			"count": reaction.count,
			"reactedByUser": reaction.reacted_by_user,
		})).collect::<Vec<_>>(),
		"preview": preview_text(message),
		"request": message.request,
	})
}

/// The name a status has on the wire.
pub fn status_name(status: ChatMessageStatus) -> &'static str {
	match status {
		ChatMessageStatus::Preparing => "Preparing",
		ChatMessageStatus::Sent => "Sent",
		ChatMessageStatus::Delivered => "Delivered",
		ChatMessageStatus::Read => "Read",
		ChatMessageStatus::Failed => "Failed",
	}
}

/// The surface view of a message body. Every field is kept, so the surface
/// draws what the host holds.
pub fn body_json(body: &ChatMessageBody) -> Value {
	match body {
		ChatMessageBody::Text { text } => json!({ "type": "Text", "text": text }),
		ChatMessageBody::RichText { text, media } => json!({
			"type": "RichText",
			"text": text,
			"media": media.iter().map(media_json).collect::<Vec<_>>(),
		}),
		ChatMessageBody::Event(event) => json!({
			"type": "Event",
			"event": event_name(*event),
			"text": event_line(*event),
		}),
		ChatMessageBody::Call(call) => json!({
			"type": "Call",
			"video": call.video,
			"answeredAt": call.answered_at,
			"closed": call.closed.map(|closed| json!({ "at": closed.at, "byUser": closed.by_user })),
			"text": call_line(call),
		}),
		// A JSON number cannot hold every `u128`, so the amount is a string.
		ChatMessageBody::Payment(payment) => json!({
			"type": "Payment",
			"amount": payment.amount.to_string(),
			"legacy": payment.legacy,
		}),
		// This page has no renderer, so it shows the `alt` of a custom message.
		ChatMessageBody::Custom { message, alt } => json!({
			"type": "Custom",
			"messageType": message.message_type,
			"payload": hex::encode(&message.payload),
			"alt": alt,
		}),
	}
}

/// The name an event has on the wire.
fn event_name(event: ChatContactEvent) -> &'static str {
	match event {
		ChatContactEvent::RequestAccepted => "RequestAccepted",
		ChatContactEvent::ContactAdded => "ContactAdded",
		ChatContactEvent::LeftChat => "LeftChat",
		ChatContactEvent::Unavailable => "Unavailable",
		ChatContactEvent::Unsupported => "Unsupported",
	}
}

fn media_json(item: &ChatMediaItem) -> Value {
	let (kind, resource, file_name) = match item {
		ChatMediaItem::Image { resource, .. } => ("Image", resource, None),
		ChatMediaItem::Video { resource, .. } => ("Video", resource, None),
		ChatMediaItem::File { resource, file_name, .. } => ("File", resource, Some(file_name)),
	};
	let meta = media_meta(item);
	json!({
		"kind": kind,
		"resource": resource_json(resource),
		"fileName": file_name,
		"meta": {
			"mimeType": meta.mime_type,
			"sizeBytes": meta.size_bytes,
			"width": meta.width,
			"height": meta.height,
			"placeholder": meta.placeholder,
			"alt": meta.alt,
		},
	})
}

/// The variant name a transcript reader matches on.
fn variant_name(content: &ChatMessageContent) -> &'static str {
	match content {
		ChatMessageContent::Text { .. } => "Text",
		ChatMessageContent::RichText(_) => "RichText",
		ChatMessageContent::Actions(_) => "Actions",
		ChatMessageContent::File(_) => "File",
		ChatMessageContent::Reaction(_) => "Reaction",
		ChatMessageContent::ReactionRemoved(_) => "ReactionRemoved",
		ChatMessageContent::Custom(_) => "Custom",
	}
}

fn body_variant(body: &ChatMessageBody) -> &'static str {
	match body {
		ChatMessageBody::Text { .. } => "Text",
		ChatMessageBody::RichText { .. } => "RichText",
		ChatMessageBody::Event(_) => "Event",
		ChatMessageBody::Call(_) => "Call",
		ChatMessageBody::Payment(_) => "Payment",
		ChatMessageBody::Custom { .. } => "Custom",
	}
}

#[async_trait]
impl ChatPlatform for CliChatHost {
	async fn create_chat_room(
		&self,
		_product: &ProductContext,
		request: HostChatCreateRoomRequest,
	) -> Result<HostChatCreateRoomResponse, HostChatCreateRoomError> {
		let mut state = self.lock();
		// A room that exists keeps what it declared.
		let status = if state.rooms.contains_key(&request.room_id) {
			ChatRoomRegistrationStatus::Exists
		} else {
			let room = RoomRecord::new(
				ChatRoomParticipation::RoomHost,
				request.name.clone(),
				request.icon.clone(),
			);
			let event = with_kind("room", room_json(&request.room_id, &room));
			state.rooms.insert(request.room_id.clone(), room);
			Self::republish(&mut state);
			Self::broadcast(&mut state, event);
			ChatRoomRegistrationStatus::New
		};
		drop(state);
		self.record_registration("room", &request.room_id, &request.name, &request.icon);
		Ok(HostChatCreateRoomResponse { status })
	}

	async fn register_chat_bot(
		&self,
		_product: &ProductContext,
		request: HostChatRegisterBotRequest,
	) -> Result<HostChatRegisterBotResponse, HostChatRegisterBotError> {
		let mut state = self.lock();
		let status = if state.bots.insert(request.bot_id.clone()) {
			ChatBotRegistrationStatus::New
		} else {
			ChatBotRegistrationStatus::Exists
		};
		drop(state);
		self.record_registration("bot", &request.bot_id, &request.name, &request.icon);
		Ok(HostChatRegisterBotResponse { status })
	}

	async fn post_chat_message(
		&self,
		_product: &ProductContext,
		request: HostChatPostMessageRequest,
	) -> Result<HostChatPostMessageResponse, HostChatPostMessageError> {
		// A room this host never created is not one it can store against.
		let message_id = self.accept_legacy(&request).ok_or_else(|| {
			HostChatPostMessageError::Unknown { reason: unknown_room(&request.room_id) }
		})?;
		Ok(HostChatPostMessageResponse { message_id })
	}

	async fn set_chat_room_footer(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomFooterRequest,
	) -> Result<(), GenericError> {
		self.declare("footer", &request.room_id, &request, |room| {
			room.footer = request.footer;
		})
	}

	fn subscribe_chat_rooms(
		&self,
		_product: &ProductContext,
	) -> BoxStream<'static, Result<HostChatListSubscribeItem, GenericError>> {
		let mut state = self.lock();
		let snapshot = Self::room_list(&state);
		let (sender, receiver) = mpsc::unbounded();
		state.subscribers.push(sender);
		// The snapshot first, then every replacement, so a product that
		// subscribes before creating a room still sees the room it creates.
		stream::once(async move { Ok(snapshot) }).chain(receiver.map(Ok)).boxed()
	}

	async fn read_chat_room_config(
		&self,
		_product: &ProductContext,
		room_id: String,
	) -> Result<ChatRoomConfig, GenericError> {
		Ok(self.room_config(&room_id).unwrap_or_default())
	}

	async fn set_chat_room_config(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomConfigRequest,
	) -> Result<(), GenericError> {
		self.declare("config", &request.room_id, &request, |room| {
			room.config = request.config.clone();
		})
	}

	async fn set_chat_room_metadata(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomMetadataRequest,
	) -> Result<(), GenericError> {
		self.declare("metadata", &request.room_id, &request, |room| {
			room.metadata = request.metadata.clone();
		})
	}

	async fn set_chat_room_preview(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomPreviewRequest,
	) -> Result<(), GenericError> {
		self.declare("preview", &request.room_id, &request, |room| {
			room.preview = Some(request.preview.clone());
		})
	}

	async fn delete_chat_room(
		&self,
		_product: &ProductContext,
		request: HostChatDeleteRoomRequest,
	) -> Result<(), GenericError> {
		let mut state = self.lock();
		// Idempotent: a room that is gone is deleted.
		if state.rooms.remove(&request.room_id).is_some() {
			Self::republish(&mut state);
			Self::broadcast(
				&mut state,
				json!({ "kind": "roomDeleted", "roomId": request.room_id }),
			);
		}
		drop(state);
		self.record_declaration("deleteRoom", &request.room_id, &request);
		Ok(())
	}

	async fn update_chat_feed(
		&self,
		_product: &ProductContext,
		request: HostChatUpdateFeedRequest,
		persist: bool,
	) -> Result<(), HostChatUpdateFeedError> {
		let mut state = self.lock();
		let room = state
			.rooms
			.get_mut(&request.room_id)
			.ok_or(HostChatUpdateFeedError::UnknownRoom)?;
		if let Some(index) = apply_feed(room, &request.update, persist)? {
			Self::announce_message(&mut state, &request.room_id, index);
		}
		drop(state);
		let (update, message_id) = feed_target(&request.update);
		self.record(json!({
			"kind": "feed",
			"roomId": request.room_id,
			"persist": persist,
			"update": update,
			"messageId": message_id,
			"payload": hex::encode(request.update.encode()),
		}));
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::Decode;
	use std::fs::read_to_string;
	use truapi::{
		latest::{
			ChatCallClose, ChatCustomMessage, ChatPaymentRecord, ChatPreviewReaction,
			HostChatSetRoomFooterRequest,
		},
		platform::ProductId,
	};

	/// The reserved participant id of the product itself.
	const PRODUCT: &str = "product";

	fn product() -> ProductContext {
		ProductContext::new(ProductId::for_test("chat.dot"))
	}

	fn text(text: &str) -> ChatMessageContent {
		ChatMessageContent::Text { text: text.to_string() }
	}

	fn body(text: &str) -> ChatMessageBody {
		ChatMessageBody::Text { text: text.to_string() }
	}

	fn room(room_id: &str) -> HostChatCreateRoomRequest {
		HostChatCreateRoomRequest {
			room_id: room_id.to_string(),
			name: "Support".to_string(),
			icon: String::new(),
		}
	}

	fn source_config() -> ChatRoomConfig {
		ChatRoomConfig { product_source: true, report_seen: true, ..ChatRoomConfig::default() }
	}

	fn create(host: &CliChatHost, request: HostChatCreateRoomRequest) {
		futures::executor::block_on(host.create_chat_room(&product(), request))
			.expect("a room is created");
	}

	fn configure(host: &CliChatHost, room_id: &str, config: ChatRoomConfig) {
		futures::executor::block_on(host.set_chat_room_config(
			&product(),
			HostChatSetRoomConfigRequest { room_id: room_id.to_string(), config },
		))
		.expect("the room exists");
	}

	/// A host with the room `support` the host keeps and the room `kept`
	/// the product keeps.
	fn host_with_rooms(transcript: Option<PathBuf>) -> Arc<CliChatHost> {
		let host = CliChatHost::new(transcript);
		create(&host, room("support"));
		create(&host, room("kept"));
		configure(&host, "kept", source_config());
		host
	}

	fn message(message_id: &str, origin: &str, text: &str) -> ChatMessage {
		ChatMessage {
			message_id: message_id.to_string(),
			origin: ChatMessageOrigin { participant_id: origin.to_string() },
			timestamp: 1_767_225_600_000,
			body: body(text),
			status: ChatMessageStatus::Sent,
			reply_to: None,
			reply_preview: None,
			edited_at: None,
			reactions: Vec::new(),
			request: false,
		}
	}

	fn feed(
		host: &CliChatHost,
		room_id: &str,
		update: ChatFeedUpdate,
		persist: bool,
	) -> Result<(), HostChatUpdateFeedError> {
		futures::executor::block_on(host.update_chat_feed(
			&product(),
			HostChatUpdateFeedRequest { room_id: room_id.to_string(), update },
			persist,
		))
	}

	fn room_view(host: &CliChatHost, room_id: &str) -> Value {
		host.observe()
			.0
			.rooms
			.into_iter()
			.find(|room| room["roomId"] == room_id)
			.expect("the room is in the snapshot")
	}

	fn drain(events: &mut mpsc::UnboundedReceiver<Value>) -> Vec<Value> {
		std::iter::from_fn(|| events.try_recv().ok()).collect()
	}

	fn transcript_lines(path: &std::path::Path) -> Vec<Value> {
		read_to_string(path)
			.expect("the transcript is readable")
			.lines()
			.map(|line| serde_json::from_str(line).expect("one JSON object per line"))
			.collect()
	}

	#[test]
	fn a_message_needs_a_room_this_host_created() {
		let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
		let host = CliChatHost::new(Some(transcript.path().to_path_buf()));

		let posted = futures::executor::block_on(host.post_chat_message(
			&product(),
			HostChatPostMessageRequest {
				room_id: "support".to_string(),
				payload: text("hello"),
				alt: None,
			},
		));

		assert!(matches!(posted, Err(HostChatPostMessageError::Unknown { .. })));
		// A refused message is not one this host was willing to store, so it
		// must not appear in the record a battery reads.
		assert_eq!(read_to_string(transcript.path()).expect("the transcript is readable"), "");
	}

	#[test]
	fn a_stored_message_is_recorded_as_the_host_received_it() {
		let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
		let host = CliChatHost::new(Some(transcript.path().to_path_buf()));
		create(&host, room("support"));

		let payload = text("line one\nline two");
		let posted = futures::executor::block_on(host.post_chat_message(
			&product(),
			HostChatPostMessageRequest {
				room_id: "support".to_string(),
				payload: payload.clone(),
				alt: Some("Two lines".to_string()),
			},
		))
		.expect("a message posts into a room this host created");

		assert_eq!(posted.message_id, "m1");
		let recorded = transcript_lines(transcript.path()).pop().expect("the message line");
		assert_eq!(recorded["kind"], "message");
		assert_eq!(recorded["messageId"], "m1");
		assert_eq!(recorded["roomId"], "support");
		assert_eq!(recorded["variant"], "Text");
		// The payload as bytes, so a difference between what a product sent
		// and what the host received cannot hide behind a rendering.
		assert_eq!(recorded["payload"], hex::encode(payload.encode()));
		assert_eq!(recorded["alt"], "Two lines");
	}

	#[test]
	fn a_room_appears_in_the_list_a_subscriber_already_holds() {
		let host = CliChatHost::new(None);
		let mut rooms = host.subscribe_chat_rooms(&product());

		let snapshot = futures::executor::block_on(rooms.next())
			.expect("a subscription emits its snapshot")
			.expect("the snapshot is not an error");
		assert!(snapshot.rooms.is_empty());

		create(&host, room("support"));

		let replacement = futures::executor::block_on(rooms.next())
			.expect("creating a room republishes the list")
			.expect("the replacement is not an error");
		assert_eq!(replacement.rooms.len(), 1);
		assert_eq!(replacement.rooms[0].room_id, "support");
	}

	/// A surface that opens before the worker creates its room must still
	/// learn about the room, and one that opens after must not be told twice.
	#[test]
	fn an_observer_sees_changes_after_its_snapshot_exactly_once() {
		let host = CliChatHost::new(None);
		let (before, mut early) = host.observe();
		assert!(before.rooms.is_empty());

		create(&host, room("support"));
		let (after, mut late) = host.observe();
		let posted = host.post_user_message("support", body("hi"), None).expect("the room exists");

		let room = early.try_recv().expect("the room event");
		assert_eq!(room["kind"], "room");
		assert_eq!(room["roomId"], "support");
		assert_eq!(room["name"], "Support");
		assert_eq!(room["participatingAs"], "RoomHost");
		let message = early.try_recv().expect("the message event");
		assert_eq!(message["kind"], "message");
		assert_eq!(message["message"]["messageId"], posted.message_id);

		assert_eq!(after.rooms.len(), 1);
		assert_eq!(after.rooms[0]["name"], "Support");
		let only = late.try_recv().expect("the message event");
		assert_eq!(only, message);
		assert!(late.try_recv().is_err(), "the room was in the snapshot");
	}

	/// A product correlates an action trigger by message id, so an id the
	/// person's message took must never be handed to a product message too.
	#[test]
	fn user_and_product_messages_share_one_id_sequence() {
		let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
		let host = CliChatHost::new(Some(transcript.path().to_path_buf()));
		create(&host, room("support"));
		let post = |content| {
			futures::executor::block_on(host.post_chat_message(
				&product(),
				HostChatPostMessageRequest {
					room_id: "support".to_string(),
					payload: content,
					alt: None,
				},
			))
			.expect("the room exists")
			.message_id
		};

		let first = post(text("welcome"));
		let second =
			host.post_user_message("support", body("hello"), None).expect("the room exists");
		let third = post(text("hi there"));
		assert_eq!(
			(first.as_str(), second.message_id.as_str(), third.as_str()),
			("m1", "m2", "m3")
		);
		assert_eq!(second.origin.participant_id, USER);

		let recorded = transcript_lines(transcript.path());
		assert_eq!(recorded[2]["author"], "person");
		assert_eq!(recorded[2]["messageId"], "m2");
		assert_eq!(recorded[2]["payload"], hex::encode(body("hello").encode()));

		assert!(host.post_user_message("elsewhere", body("lost"), None).is_err());
		assert_eq!(host.observe().0.messages.len(), 1);
	}

	#[test]
	fn a_room_the_product_keeps_takes_no_message_from_the_user_through_the_host() {
		let host = host_with_rooms(None);
		assert!(host.post_user_message("kept", body("hi"), None).is_err());
	}

	#[test]
	fn a_room_that_exists_keeps_what_it_declared() {
		let host = CliChatHost::new(None);
		create(&host, room("kept"));
		configure(&host, "kept", source_config());

		let again = futures::executor::block_on(host.create_chat_room(
			&product(),
			HostChatCreateRoomRequest { name: "Renamed".to_string(), ..room("kept") },
		))
		.expect("an existing room is found again");

		assert_eq!(again.status, ChatRoomRegistrationStatus::Exists);
		assert_eq!(host.room_config("kept"), Some(source_config()));
		assert_eq!(room_view(&host, "kept")["name"], "Support");
	}

	#[test]
	fn the_configuration_of_a_room_reads_back_or_defaults() {
		let host = host_with_rooms(None);
		let read = |room_id: &str| {
			futures::executor::block_on(host.read_chat_room_config(&product(), room_id.to_string()))
				.expect("a configuration is always read")
		};
		assert_eq!(read("kept"), source_config());
		assert_eq!(read("support"), ChatRoomConfig::default());
		assert_eq!(read("never-created"), ChatRoomConfig::default());
	}

	#[test]
	fn metadata_replaces_the_name_and_description() {
		let host = host_with_rooms(None);
		let (_, mut events) = host.observe();
		futures::executor::block_on(host.set_chat_room_metadata(
			&product(),
			HostChatSetRoomMetadataRequest {
				room_id: "support".to_string(),
				metadata: ChatRoomMetadata {
					name: "Help desk".to_string(),
					description: Some("We answer".to_string()),
				},
			},
		))
		.expect("the room exists");

		let event = events.try_recv().expect("the room event");
		assert_eq!(event["name"], "Help desk");
		assert_eq!(event["description"], "We answer");
	}

	#[test]
	fn the_list_row_takes_the_preview_and_defaults_the_rest() {
		let host = host_with_rooms(None);
		feed(&host, "support", ChatFeedUpdate::New(message("p-1", PRODUCT, "newest")), true)
			.expect("the room exists");
		// With nothing declared: the newest text and the host's unread count.
		let list = &room_view(&host, "support")["list"];
		assert_eq!(list["text"], "newest");
		assert_eq!(list["origin"], PRODUCT);
		assert_eq!(list["badge"], json!({ "type": "UnreadCount", "count": 1 }));
		assert_eq!(list["unseenReactions"], 0);

		let preview = |preview: ChatRoomPreview| {
			futures::executor::block_on(host.set_chat_room_preview(
				&product(),
				HostChatSetRoomPreviewRequest { room_id: "support".to_string(), preview },
			))
			.expect("the room exists");
			room_view(&host, "support")["list"].clone()
		};
		let list = preview(ChatRoomPreview {
			timestamp: Some(42),
			badge: Some(ChatRoomBadge::UnreadCount { count: 7 }),
			last_message: Some(message("c-1", "contact", "from the product")),
			last_reaction: Some(ChatPreviewReaction {
				emoji: "+1".to_string(),
				by_user: false,
				message: Some(message("c-1", "contact", "from the product")),
				removed: true,
			}),
			unseen_reactions: 2,
			last_call: Some(ChatMessage {
				body: ChatMessageBody::Call(ChatCallRecord {
					video: false,
					answered_at: Some(2),
					closed: Some(ChatCallClose { at: 3, by_user: true }),
				}),
				..message("call-1", USER, "")
			}),
		});
		assert_eq!(list["timestamp"], 42);
		assert_eq!(list["badge"], json!({ "type": "UnreadCount", "count": 7 }));
		assert_eq!(list["text"], "from the product");
		assert_eq!(list["origin"], "contact");
		assert_eq!(list["lastCall"], json!({ "messageId": "call-1", "text": "Audio call, ended" }));
		assert_eq!(
			list["lastReaction"],
			json!({ "emoji": "+1", "byUser": false, "messageId": "c-1", "removed": true })
		);
		assert_eq!(list["unseenReactions"], 2);

		// The variant that hides the badge, by its wire index rather than
		// its name.
		let hidden = ChatRoomBadge::decode(&mut &[0u8][..]).expect("variant 0 hides the badge");
		let list = preview(ChatRoomPreview {
			timestamp: None,
			badge: Some(hidden),
			last_message: None,
			last_reaction: None,
			unseen_reactions: 0,
			last_call: None,
		});
		assert_eq!(list["badge"], Value::Null);
		assert_eq!(list["text"], "newest");
		let list = preview(ChatRoomPreview {
			timestamp: None,
			badge: Some(ChatRoomBadge::UnreadCount { count: 0 }),
			last_message: None,
			last_reaction: None,
			unseen_reactions: 0,
			last_call: None,
		});
		assert_eq!(list["badge"], Value::Null, "a count of 0 shows nothing");
	}

	#[test]
	fn deleting_a_room_removes_it_and_is_idempotent() {
		let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
		let host = host_with_rooms(Some(transcript.path().to_path_buf()));
		let mut rooms = host.subscribe_chat_rooms(&product());
		futures::executor::block_on(rooms.next());
		let (_, mut events) = host.observe();
		let delete = |room_id: &str| {
			futures::executor::block_on(host.delete_chat_room(
				&product(),
				HostChatDeleteRoomRequest { room_id: room_id.to_string() },
			))
		};

		delete("support").expect("a room is deleted");
		delete("support").expect("a room that is gone is deleted");

		assert_eq!(drain(&mut events), vec![json!({ "kind": "roomDeleted", "roomId": "support" })]);
		let list = futures::executor::block_on(rooms.next())
			.expect("the list is republished")
			.expect("not an error");
		assert_eq!(
			list.rooms.iter().map(|room| room.room_id.as_str()).collect::<Vec<_>>(),
			["kept"]
		);
		assert!(host.room_config("support").is_none());
		let kinds: Vec<_> = transcript_lines(transcript.path())
			.into_iter()
			.map(|line| line["kind"].clone())
			.collect();
		assert_eq!(kinds, ["room", "room", "config", "deleteRoom", "deleteRoom"]);
	}

	#[test]
	fn the_store_of_a_room_takes_every_feed_update() {
		let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
		let host = host_with_rooms(Some(transcript.path().to_path_buf()));
		let user = host.post_user_message("support", body("mine"), None).expect("the room exists");
		let (_, mut events) = host.observe();

		feed(&host, "support", ChatFeedUpdate::New(message("p-1", PRODUCT, "first")), true)
			.expect("a new message is stored");
		assert_eq!(
			feed(&host, "support", ChatFeedUpdate::New(message("p-1", PRODUCT, "again")), true),
			Err(HostChatUpdateFeedError::DuplicateMessage)
		);
		feed(
			&host,
			"support",
			ChatFeedUpdate::Edited {
				message_id: "p-1".to_string(),
				body: body("first, edited"),
				edited_at: 99,
			},
			true,
		)
		.expect("the product edits its message");
		assert_eq!(
			feed(
				&host,
				"support",
				ChatFeedUpdate::Edited {
					message_id: user.message_id.clone(),
					body: body("not yours"),
					edited_at: 99,
				},
				true
			),
			Err(HostChatUpdateFeedError::NotOwner)
		);
		for participant_id in ["p1", USER, "p1", "p2"] {
			let update = ChatFeedUpdate::ReactionPlaced {
				message_id: "p-1".to_string(),
				participant_id: participant_id.to_string(),
				emoji: "+1".to_string(),
			};
			feed(&host, "support", update, true).expect("a reaction is stored");
		}
		feed(
			&host,
			"support",
			ChatFeedUpdate::ReactionRemoved {
				message_id: "p-1".to_string(),
				participant_id: "p2".to_string(),
				emoji: "+1".to_string(),
			},
			true,
		)
		.expect("a reaction is removed");
		feed(
			&host,
			"support",
			ChatFeedUpdate::StatusChanged {
				message_id: "p-1".to_string(),
				status: ChatMessageStatus::Read,
			},
			true,
		)
		.expect("a status changes");

		let last = drain(&mut events).pop().expect("message events");
		assert_eq!(last["persisted"], true);
		let stored = &last["message"];
		assert_eq!(stored["body"]["text"], "first, edited");
		assert_eq!(stored["editedAt"], 99);
		assert_eq!(stored["status"], "Read");
		// One per participant, the repeat of p1 counted once.
		assert_eq!(
			stored["reactions"],
			json!([{ "emoji": "+1", "count": 2, "reactedByUser": true }])
		);

		// A call that ended is drawn again in place, without an edit mark.
		let call = ChatMessage {
			body: ChatMessageBody::Call(ChatCallRecord {
				video: false,
				answered_at: Some(5),
				closed: Some(ChatCallClose { at: 9, by_user: true }),
			}),
			..message("p-1", PRODUCT, "")
		};
		feed(&host, "support", ChatFeedUpdate::Replaced(call), true).expect("replaced");
		let replaced = drain(&mut events).pop().expect("the replace event");
		assert_eq!(replaced["message"]["body"]["type"], "Call");
		assert_eq!(replaced["message"]["body"]["text"], "Audio call, ended");
		assert_eq!(replaced["message"]["editedAt"], Value::Null);
		assert_eq!(replaced["message"]["reactions"], json!([]));
		assert_eq!(
			feed(&host, "support", ChatFeedUpdate::Replaced(message("p-9", PRODUCT, "")), true),
			Err(HostChatUpdateFeedError::UnknownMessage)
		);
		assert_eq!(
			feed(&host, "gone", ChatFeedUpdate::Replaced(message("p-1", PRODUCT, "")), true),
			Err(HostChatUpdateFeedError::UnknownRoom)
		);

		let feed_lines: Vec<_> = transcript_lines(transcript.path())
			.into_iter()
			.filter(|line| line["kind"] == "feed")
			.collect();
		assert_eq!(feed_lines.len(), 9, "only the updates the host applied");
		assert_eq!(feed_lines[0]["update"], "New");
		assert_eq!(feed_lines[0]["messageId"], "p-1");
		assert_eq!(feed_lines[0]["persist"], true);
		assert_eq!(
			feed_lines[0]["payload"],
			hex::encode(ChatFeedUpdate::New(message("p-1", PRODUCT, "first")).encode())
		);
	}

	#[test]
	fn a_feed_update_of_a_room_the_product_keeps_is_for_the_screen_only() {
		let host = host_with_rooms(None);
		let (_, mut events) = host.observe();

		// The page cache never saw it: nothing to change, and no error.
		feed(&host, "kept", ChatFeedUpdate::Replaced(message("old", PRODUCT, "")), false)
			.expect("an update for a message off screen is fine");
		assert!(drain(&mut events).is_empty());

		feed(&host, "kept", ChatFeedUpdate::New(message("s-1", PRODUCT, "hi")), false)
			.expect("a new message goes to the screen");
		let mut summary = message("s-2", USER, "with reactions");
		summary.reactions = vec![ChatReactionSummary {
			emoji: "+1".to_string(),
			count: 3,
			reacted_by_user: false,
			unseen: false,
		}];
		host.cache_page("kept", &[summary]);
		feed(
			&host,
			"kept",
			ChatFeedUpdate::ReactionPlaced {
				message_id: "s-2".to_string(),
				participant_id: USER.to_string(),
				emoji: "+1".to_string(),
			},
			false,
		)
		.expect("a reaction goes to the screen");

		let events = drain(&mut events);
		assert_eq!(events.len(), 2);
		assert_eq!(events[0]["persisted"], false);
		assert_eq!(events[0]["message"]["messageId"], "s-1");
		assert_eq!(
			events[1]["message"]["reactions"],
			json!([{ "emoji": "+1", "count": 4, "reactedByUser": true }])
		);
		// The product's own badge rules: nothing unless it set one.
		assert_eq!(events[1]["list"]["badge"], Value::Null);
		assert!(host.message("kept", "s-2").is_some());
		assert!(host.observe().0.messages.is_empty(), "the snapshot holds only the host's store");
	}

	#[test]
	fn the_footer_is_kept() {
		let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
		let host = host_with_rooms(Some(transcript.path().to_path_buf()));
		futures::executor::block_on(host.set_chat_room_footer(
			&product(),
			HostChatSetRoomFooterRequest {
				room_id: "support".to_string(),
				footer: ChatRoomFooter::Empty,
			},
		))
		.expect("the room exists");

		assert_eq!(room_view(&host, "support")["footer"], json!({ "type": "Empty" }));
		// The default footer of a room that declared none.
		assert_eq!(room_view(&host, "kept")["footer"], json!({ "type": "TextInput" }));

		let kinds: Vec<_> = transcript_lines(transcript.path())
			.into_iter()
			.map(|line| line["kind"].as_str().unwrap_or_default().to_string())
			.filter(|kind| kind != "room")
			.collect();
		assert_eq!(kinds, ["config", "footer"]);
	}

	#[test]
	fn the_newest_seen_message_clears_the_unread_count() {
		let host = host_with_rooms(None);
		for id in ["p-1", "p-2"] {
			feed(&host, "support", ChatFeedUpdate::New(message(id, PRODUCT, id)), true)
				.expect("stored");
		}
		assert_eq!(room_view(&host, "support")["list"]["badge"]["count"], 2);

		assert_eq!(host.mark_seen("support", "p-1"), Ok(false));
		assert_eq!(room_view(&host, "support")["list"]["badge"]["count"], 1);
		host.mark_seen("support", "p-2").expect("known");
		assert_eq!(room_view(&host, "support")["list"]["badge"], Value::Null);
		assert_eq!(room_view(&host, "support")["lastSeen"], "p-2");
		assert!(host.mark_seen("support", "nope").is_err());
		assert!(host.mark_seen("kept", "nope").is_err());
	}

	#[test]
	fn the_answer_to_a_user_action_shows_on_the_screen() {
		let host = host_with_rooms(None);
		host.cache_page("kept", &[message("s-1", USER, "first")]);
		let (_, mut events) = host.observe();
		let created = message("s-2", USER, "hello");

		host.apply_source_action(
			"kept",
			&ChatUserAction::NewMessage {
				body: body("hello"),
				reply_to: None,
				message_id: None,
				timestamp: None,
			},
			&ProductChatUserActionItem::MessageCreated(created.clone()),
		);
		let acknowledged = ProductChatUserActionItem::Acknowledged;
		host.apply_source_action(
			"kept",
			&ChatUserAction::EditMessage { message_id: "s-1".to_string(), body: body("edited") },
			&acknowledged,
		);
		host.apply_source_action(
			"kept",
			&ChatUserAction::ReactionPlaced {
				message_id: "s-1".to_string(),
				emoji: "+1".to_string(),
			},
			&acknowledged,
		);
		host.apply_source_action("kept", &ChatUserAction::AcceptRequest, &acknowledged);
		host.apply_source_action(
			"kept",
			&ChatUserAction::Seen { message_id: "s-1".to_string() },
			&acknowledged,
		);

		let events = drain(&mut events);
		let messages: Vec<_> = events.iter().map(|event| event["message"].clone()).collect();
		assert_eq!(messages[0]["messageId"], "s-2");
		assert_eq!(messages[1]["body"]["text"], "edited");
		assert_eq!(messages[2]["reactions"][0]["reactedByUser"], true);
		assert_eq!(events.len(), 3, "a contact action or a seen report changes no message");
		assert_eq!(room_view(&host, "kept")["lastSeen"], "s-1");
	}

	#[test]
	fn the_list_text_of_a_message_falls_back_in_order() {
		let with = |body| ChatMessage { body, ..message("m", PRODUCT, "") };
		let meta = |alt: Option<&str>| ChatMediaMeta {
			mime_type: "image/png".to_string(),
			size_bytes: None,
			width: None,
			height: None,
			placeholder: None,
			alt: alt.map(str::to_string),
			duration_ms: None,
		};
		let image = |alt| ChatMediaItem::Image { resource: hop(), meta: meta(alt) };
		assert_eq!(preview_text(&with(body("hi"))), "hi");
		assert_eq!(
			preview_text(&with(ChatMessageBody::RichText {
				text: None,
				media: vec![image(Some("A cat"))]
			})),
			"A cat"
		);
		assert_eq!(
			preview_text(&with(ChatMessageBody::RichText { text: None, media: vec![image(None)] })),
			SENT_A_MESSAGE
		);
		assert_eq!(
			preview_text(&with(ChatMessageBody::Event(ChatContactEvent::RequestAccepted))),
			"Request accepted"
		);
		assert_eq!(
			preview_text(&with(ChatMessageBody::Call(ChatCallRecord {
				video: true,
				answered_at: None,
				closed: Some(ChatCallClose { at: 1, by_user: false }),
			}))),
			"Video call, not answered"
		);
		assert_eq!(
			preview_text(&with(ChatMessageBody::Payment(ChatPaymentRecord {
				amount: 1_500,
				legacy: false,
			}))),
			"Payment of 1500"
		);
		let custom = |alt: Option<&str>| ChatMessageBody::Custom {
			message: ChatCustomMessage { message_type: "poll".to_string(), payload: vec![1] },
			alt: alt.map(str::to_string),
		};
		assert_eq!(preview_text(&with(custom(Some("A poll")))), "A poll");
		assert_eq!(preview_text(&with(custom(None))), SENT_A_MESSAGE);
	}

	fn hop() -> ChatResource {
		ChatResource::Hop {
			identifier: [1; 32],
			ticket: [2; 32],
			node: "wss://hop.example".to_string(),
			size: 1,
		}
	}

	#[test]
	fn a_body_is_shown_with_every_field_the_product_sent() {
		let media = vec![
			ChatMediaItem::Image {
				resource: hop(),
				meta: ChatMediaMeta {
					mime_type: "image/png".to_string(),
					size_bytes: Some(1),
					width: Some(2),
					height: Some(3),
					placeholder: Some("LKO2".to_string()),
					alt: Some("dot".to_string()),
					duration_ms: None,
				},
			},
			ChatMediaItem::File {
				resource: hop(),
				meta: ChatMediaMeta {
					mime_type: "application/pdf".to_string(),
					size_bytes: None,
					width: None,
					height: None,
					placeholder: None,
					alt: None,
					duration_ms: None,
				},
				file_name: "a.pdf".to_string(),
			},
		];
		let view = body_json(&ChatMessageBody::RichText { text: Some("b".to_string()), media });
		assert_eq!(view["type"], "RichText");
		assert_eq!(view["text"], "b");
		// The ticket stays with the host: the page gets the node and the size.
		assert_eq!(
			view["media"][0],
			json!({
				"kind": "Image",
				"resource": {
					"type": "Hop",
					"size": 1,
					"node": "wss://hop.example",
					"identifier": format!("0x{}", "01".repeat(32)),
				},
				"fileName": null,
				"meta": { "mimeType": "image/png", "sizeBytes": 1, "width": 2, "height": 3, "placeholder": "LKO2", "alt": "dot" },
			})
		);
		assert_eq!(view["media"][1]["kind"], "File");
		assert_eq!(view["media"][1]["fileName"], "a.pdf");
		assert_eq!(
			body_json(&ChatMessageBody::Call(ChatCallRecord {
				video: false,
				answered_at: Some(3),
				closed: None,
			})),
			json!({
				"type": "Call",
				"video": false,
				"answeredAt": 3,
				"closed": null,
				"text": "Audio call, ongoing",
			})
		);
		assert_eq!(
			body_json(&ChatMessageBody::Payment(ChatPaymentRecord {
				amount: u128::MAX,
				legacy: true,
			})),
			json!({ "type": "Payment", "amount": u128::MAX.to_string(), "legacy": true })
		);
		assert_eq!(
			body_json(&ChatMessageBody::Event(ChatContactEvent::LeftChat)),
			json!({ "type": "Event", "event": "LeftChat", "text": "Left the chat" })
		);
	}
}
