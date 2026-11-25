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
		ChatMediaMeta, ChatMessage, ChatMessageBody, ChatMessageOrigin, ChatMessageStatus,
		ChatReactionSummary, ChatReply, ChatResource, ChatRoom, ChatRoomBadge, ChatRoomConfig,
		ChatRoomFooter, ChatRoomMetadata, ChatRoomParticipation, ChatRoomPreview,
		ChatRoomRegistrationStatus, ChatUserAction, GenericError, HostChatCreateRoomError,
		HostChatCreateRoomRequest, HostChatDeleteRoomRequest, HostChatListSubscribeItem,
		HostChatPostMessageError, HostChatPostMessageRequest, HostChatPostMessageResponse,
		HostChatRegisterBotError, HostChatRegisterBotRequest, HostChatRegisterBotResponse,
		HostChatSetRoomFooterRequest, HostChatSetRoomMetadataRequest,
		HostChatSetRoomPreviewRequest, HostChatUpdateFeedRequest,
		ProductChatUserActionResponse,
	},
	platform::{ChatFeedOutcome, ChatPlatform, ChatRoomChangeOutcome, ProductContext},
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
			metadata: ChatRoomMetadata { name, description: None, icon: None, placeholder: None },
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
			reply: reply_to.map(|message_id| ChatReply { message_id, preview: None }),
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
			"replyTo": message.reply.as_ref().map(|reply| &reply.message_id),
		});
		let room = state.rooms.get_mut(room_id).expect("the room was just found");
		room.messages.push(StoredMessage::new(message.clone()));
		let index = room.messages.len() - 1;
		Self::announce_message(&mut state, room_id, index);
		drop(state);
		self.record(line);
		Ok(message)
	}

	/// Mark `message_id` as the newest message the user saw in `room_id`.
	pub fn mark_seen(&self, room_id: &str, message_id: &str) -> Result<(), String> {
		let mut state = self.lock();
		let room = state.rooms.get_mut(room_id).ok_or_else(|| unknown_room(room_id))?;
		room.position(message_id).ok_or_else(|| unknown_message(message_id))?;
		room.last_seen = Some(message_id.to_string());
		let event = with_kind("room", room_json(room_id, room));
		Self::broadcast(&mut state, event);
		Ok(())
	}

	/// Apply what the product answered to a user action in a room it keeps,
	/// so the screen shows the outcome.
	pub fn apply_source_action(
		&self,
		room_id: &str,
		action: &ChatUserAction,
		answer: &ProductChatUserActionResponse,
	) {
		let mut state = self.lock();
		let Some(room) = state.rooms.get_mut(room_id) else {
			return;
		};
		if let ChatUserAction::DeleteMessage { message_id } = action {
			if let Some(index) = room.position(message_id) {
				room.messages.remove(index);
				Self::announce_deletion(&mut state, room_id, message_id);
			}
			return;
		}
		let changed = match (action, answer) {
			(
				ChatUserAction::NewMessage { .. },
				ProductChatUserActionResponse::MessageCreated(message),
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
			(_, ProductChatUserActionResponse::MessageCreated(_)) => None,
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
			(ChatUserAction::DeleteMessage { .. }, _) => unreachable!("handled above"),
			// A send the product only acknowledged changes no message the
			// screen holds, and the page sends no payment or contact action.
			(
				ChatUserAction::NewMessage { .. } |
				ChatUserAction::SendPayment |
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

	/// Tell every observer a message left a room, with the chat-list view of
	/// its room, which the deletion can change.
	fn announce_deletion(state: &mut State, room_id: &str, message_id: &str) {
		let Some(room) = state.rooms.get(room_id) else {
			return;
		};
		let event = json!({
			"kind": "messageDeleted",
			"roomId": room_id,
			"messageId": message_id,
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
			"variant": <&str>::from(&request.payload),
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

/// What a feed update changed in a room.
enum FeedChange {
	/// The message at this index is new or changed.
	Message(usize),
	/// The message with this id left the room.
	Deleted(String),
}

/// Apply one feed update to `room`. The answer is what changed, or the outcome
/// that names why the host did not apply it. With `persist` the room is the
/// host's store and the host refuses what a store cannot hold;
/// without it the update is for the screen only, and an update for a message
/// the cache never saw changes nothing.
fn apply_feed(
	room: &mut RoomRecord,
	update: &ChatFeedUpdate,
	persist: bool,
) -> Result<Option<FeedChange>, ChatFeedOutcome> {
	let missing = || if persist { Err(ChatFeedOutcome::UnknownMessage) } else { Ok(None) };
	let index = match update {
		ChatFeedUpdate::New(message) => {
			return match room.position(&message.message_id) {
				Some(_) if persist => Err(ChatFeedOutcome::DuplicateMessage),
				Some(index) => {
					room.messages[index] = StoredMessage::new(message.clone());
					Ok(Some(FeedChange::Message(index)))
				},
				None => {
					room.messages.push(StoredMessage::new(message.clone()));
					Ok(Some(FeedChange::Message(room.messages.len() - 1)))
				},
			};
		},
		ChatFeedUpdate::Edited { message_id, .. } |
		ChatFeedUpdate::Deleted { message_id } |
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
	if let ChatFeedUpdate::Deleted { message_id } = update {
		// In the host store, the user's words are the user's own.
		if persist && room.messages[index].message.origin.participant_id == USER {
			return Err(ChatFeedOutcome::NotOwner);
		}
		room.messages.remove(index);
		return Ok(Some(FeedChange::Deleted(message_id.clone())));
	}
	let stored = &mut room.messages[index];
	match update {
		ChatFeedUpdate::New(_) | ChatFeedUpdate::Deleted { .. } => unreachable!("handled above"),
		ChatFeedUpdate::Edited { body, edited_at, .. } => {
			// In the host's store the user's words are the user's own.
			if persist && stored.message.origin.participant_id == USER {
				return Err(ChatFeedOutcome::NotOwner);
			}
			stored.message.body = body.clone();
			stored.message.edited_at = Some(*edited_at);
		},
		// The host draws the message again in the same place. Only an edit adds
		// an edit mark. In the host store, a replacement keeps its author, as
		// an edit does.
		ChatFeedUpdate::Replaced(message) => {
			if persist && stored.message.origin.participant_id == USER {
				return Err(ChatFeedOutcome::NotOwner);
			}
			let origin = stored.message.origin.clone();
			stored.message = message.clone();
			if persist {
				stored.message.origin = origin;
			}
		},
		// In the host store, the user reacts only through the host.
		ChatFeedUpdate::ReactionPlaced { participant_id, .. } |
		ChatFeedUpdate::ReactionRemoved { participant_id, .. }
			if persist && participant_id == USER =>
		{
			return Err(ChatFeedOutcome::NotOwner);
		},
		ChatFeedUpdate::ReactionPlaced { participant_id, emoji, .. } => {
			stored.react(participant_id, emoji, true, persist);
		},
		ChatFeedUpdate::ReactionRemoved { participant_id, emoji, .. } => {
			stored.react(participant_id, emoji, false, persist);
		},
		ChatFeedUpdate::StatusChanged { status, .. } => stored.message.status = *status,
	}
	Ok(Some(FeedChange::Message(index)))
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
		ChatFeedUpdate::Deleted { message_id } => ("Deleted", message_id),
	}
}

/// `body` with the surface-protocol `kind` discriminant added.
fn with_kind(kind: &str, mut body: Value) -> Value {
	body["kind"] = Value::from(kind);
	body
}

/// The chat-list text of a message body: its text, the `alt` of its first
/// media item, the line of an event, a call or a payment, the `alt` of a
/// custom message, or a generic line.
pub fn preview_text(body: &ChatMessageBody) -> String {
	let text = match body {
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
		"placeholder": room.metadata.placeholder,
		"config": config_json(&room.config),
		"footer": footer_json(&room.footer),
		"lastSeen": room.last_seen,
		"list": list_json(room),
	})
}

/// The chat-list row of a room: the product's preview where it set one, the
/// host default of each field where it did not. The line comes from the
/// newest message the preview names, else from the newest message the host
/// holds.
fn list_json(room: &RoomRecord) -> Value {
	let preview = room.preview.as_ref();
	let last = match preview.and_then(|preview| preview.last_message.as_ref()) {
		Some(message) => Some((message.timestamp, &message.body, &message.origin)),
		None => room
			.last_message()
			.map(|message| (message.timestamp, &message.body, &message.origin)),
	};
	let reaction = preview.and_then(|preview| preview.last_reaction.as_ref());
	let badge = match preview.and_then(|preview| preview.badge.as_ref()) {
		Some(ChatRoomBadge::UnreadCount { count }) if *count > 0 => {
			json!({ "type": "UnreadCount", "count": count })
		},
		Some(ChatRoomBadge::Image { icon }) => json!({ "type": "Image", "icon": icon }),
		// An unread count of 0 shows nothing.
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
			.or_else(|| last.map(|(timestamp, ..)| timestamp)),
		"text": last.map(|(_, body, _)| preview_text(body)),
		"origin": last.map(|(.., origin)| &origin.participant_id),
		"lastReaction": reaction.map(|reaction| json!({
			"emoji": reaction.emoji,
			"byUser": reaction.by_user,
			"messageId": reaction.message.as_ref().map(|message| &message.message_id),
			"removed": reaction.removed,
		})),
		"unseenReactions": preview.map_or(0, |preview| preview.unseen_reactions),
		"badge": badge,
		"pinned": preview.is_some_and(|preview| preview.pinned),
		"activity": preview.and_then(|preview| preview.activity.as_ref()).map(|activity| json!({
			"text": activity.text,
			"ongoing": activity.ongoing,
		})),
		"previewDeclared": preview.is_some(),
	})
}

fn config_json(config: &ChatRoomConfig) -> Value {
	json!({
		"productSource": config.product_source,
		"reactionDetails": config.reaction_details,
	})
}

fn footer_json(footer: &ChatRoomFooter) -> Value {
	match footer {
		ChatRoomFooter::TextInput => json!({ "type": "TextInput" }),
		ChatRoomFooter::Empty => json!({ "type": "Empty" }),
		ChatRoomFooter::Card(card) => json!({
			"type": "Card",
			"messageType": card.message_type,
			"payload": hex::encode(&card.payload),
		}),
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
		"replyTo": message.reply.as_ref().map(|reply| &reply.message_id),
		"editedAt": message.edited_at,
		"reactions": message.reactions.iter().map(|reaction| json!({
			"emoji": reaction.emoji,
			"count": reaction.count,
			"reactedByUser": reaction.reacted_by_user,
		})).collect::<Vec<_>>(),
		"preview": preview_text(&message.body),
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

#[async_trait::async_trait]
impl ChatPlatform for CliChatHost {
	async fn create_chat_room(
		&self,
		_product: &ProductContext,
		room_id: String,
		request: HostChatCreateRoomRequest,
	) -> Result<ChatRoomRegistrationStatus, HostChatCreateRoomError> {
		let mut state = self.lock();
		// A room that exists keeps its name and its other declarations. The
		// configuration is the one of this call, for a new room and for a room
		// that exists.
		let (status, config_changed) = match state.rooms.get_mut(&room_id) {
			Some(room) => {
				let changed = room.config != request.config;
				// A room that changes its store discards the messages that it
				// stored or the pages that it cached.
				if room.config.product_source != request.config.product_source {
					room.messages.clear();
					room.last_seen = None;
				}
				room.config = request.config.clone();
				if changed {
					let event = with_kind("room", room_json(&room_id, room));
					Self::broadcast(&mut state, event);
				}
				(ChatRoomRegistrationStatus::Exists, changed)
			},
			None => {
				let mut room = RoomRecord::new(
					ChatRoomParticipation::RoomHost,
					request.name.clone(),
					request.icon.clone(),
				);
				let changed = room.config != request.config;
				room.config = request.config.clone();
				let event = with_kind("room", room_json(&room_id, &room));
				state.rooms.insert(room_id.clone(), room);
				Self::republish(&mut state);
				Self::broadcast(&mut state, event);
				(ChatRoomRegistrationStatus::New, changed)
			},
		};
		drop(state);
		self.record_registration("room", &room_id, &request.name, &request.icon);
		if config_changed {
			self.record_declaration("config", &room_id, &request.config);
		}
		Ok(status)
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
			room.footer = request.footer.clone();
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

	async fn set_chat_room_metadata(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomMetadataRequest,
	) -> Result<(), GenericError> {
		self.declare("metadata", &request.room_id, &request, |room| {
			room.metadata = request.metadata.clone();
			if let Some(icon) = &request.metadata.icon {
				room.icon_url = icon.clone();
			}
		})
	}

	async fn set_chat_room_preview(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomPreviewRequest,
	) -> Result<ChatRoomChangeOutcome, GenericError> {
		self.declare("preview", &request.room_id, &request, |room| {
			room.preview = Some(request.preview.clone());
		})?;
		Ok(ChatRoomChangeOutcome::Applied)
	}

	async fn delete_chat_room(
		&self,
		_product: &ProductContext,
		request: HostChatDeleteRoomRequest,
	) -> Result<ChatRoomChangeOutcome, GenericError> {
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
		Ok(ChatRoomChangeOutcome::Applied)
	}

	async fn update_chat_feed(
		&self,
		_product: &ProductContext,
		request: HostChatUpdateFeedRequest,
		persist: bool,
	) -> Result<ChatFeedOutcome, GenericError> {
		let mut state = self.lock();
		let room = state
			.rooms
			.get_mut(&request.room_id)
			.ok_or_else(|| GenericError { reason: unknown_room(&request.room_id) })?;
		let change = match apply_feed(room, &request.update, persist) {
			Ok(change) => change,
			// The host did not apply the update and says why.
			Err(refusal) => return Ok(refusal),
		};
		match change {
			Some(FeedChange::Message(index)) => {
				Self::announce_message(&mut state, &request.room_id, index);
			},
			Some(FeedChange::Deleted(message_id)) => {
				Self::announce_deletion(&mut state, &request.room_id, &message_id);
			},
			None => {},
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
		Ok(ChatFeedOutcome::Applied)
	}

	/// The core checks the room of each change to a room against this list.
	/// The default reads the subscription, which would leave one subscriber
	/// for each check until the next room list goes out.
	async fn list_chat_rooms(
		&self,
		_product: &ProductContext,
	) -> Result<HostChatListSubscribeItem, GenericError> {
		Ok(Self::room_list(&self.lock()))
	}
}

#[cfg(test)]
mod tests;
