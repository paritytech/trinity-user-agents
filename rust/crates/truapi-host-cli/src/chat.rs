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
//! what the product declared about it (metadata, configuration, preview,
//! footer, toolbar, suggestions, commands, activity, optional features, the
//! meta of its messages and its call), the state the user gave it (mute and
//! archive), and its messages: every message of a room the host keeps, and a
//! page cache of a room the product keeps. Each change goes to every observer
//! as the JSON the surface protocol names.

use std::{
	collections::BTreeMap,
	fs::OpenOptions,
	io::Write,
	path::PathBuf,
	sync::{Arc, Mutex},
	time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use futures::{
	StreamExt,
	channel::mpsc,
	stream::{self, BoxStream},
};
use parity_scale_codec::Encode;
use serde_json::{Value, json};
use truapi::{
	latest::{
		ChatActionHandler, ChatActionLayout, ChatActivity, ChatBotRegistrationStatus,
		ChatCommandSpec, ChatFeedUpdate, ChatMediaItem, ChatMediaMeta, ChatMessage,
		ChatMessageBody, ChatMessageContent, ChatMessageMeta, ChatMessageOrigin, ChatMessageStatus,
		ChatParticipantEntry, ChatReactionSummary, ChatResource, ChatRoom, ChatRoomBadge,
		ChatRoomCall, ChatRoomCalls, ChatRoomConfig, ChatRoomFeatures, ChatRoomFooter,
		ChatRoomMetadata, ChatRoomOrder, ChatRoomParticipation, ChatRoomPreview,
		ChatRoomRegistrationStatus, ChatSuggestion, ChatToolbarAction, ChatUserAction,
		GenericError, HostChatCreateRoomError, HostChatCreateRoomRequest,
		HostChatCreateRoomResponse, HostChatDeleteRoomRequest, HostChatListSubscribeItem,
		HostChatPostMessageError, HostChatPostMessageRequest, HostChatPostMessageResponse,
		HostChatRegisterBotError, HostChatRegisterBotRequest, HostChatRegisterBotResponse,
		HostChatSetMessageMetaRequest, HostChatSetRoomActivityRequest, HostChatSetRoomCallRequest,
		HostChatSetRoomCommandsRequest, HostChatSetRoomConfigRequest,
		HostChatSetRoomFeaturesRequest, HostChatSetRoomFooterRequest,
		HostChatSetRoomMetadataRequest, HostChatSetRoomPreviewRequest,
		HostChatSetRoomSuggestionsRequest, HostChatSetRoomToolbarRequest, HostChatUpdateFeedError,
		HostChatUpdateFeedRequest, ProductChatUserActionItem,
	},
	platform::{
		CHAT_META_KEPT_MAX, ChatBotIds, ChatFeatureNames, ChatPlatform, ChatRoomUserState,
		ChatRoomUserStates, ProductContext, async_trait,
	},
};

/// The reserved participant id of the host's user.
pub const USER: &str = "user";

/// The reserved participant id of the product itself.
pub const PRODUCT: &str = "product";

/// The list text of a message that has no text of its own.
const SENT_A_MESSAGE: &str = "Sent a message";

/// The Chat features this host serves from end to end, from
/// `CHAT_FEATURE_NAMES`. The page has no renderer and no room links, so
/// `custom_bodies`, `custom_footer` and `room_links` are not here. A P10 item
/// adds its name when the page serves it.
pub const CLI_CHAT_FEATURES: [&str; 19] = [
	"product_source_rooms",
	"reaction_details",
	"participants",
	"message_menus",
	"participant_menus",
	"toolbar",
	"suggestions",
	"commands",
	"activity",
	"inline_attachments",
	"preimage_attachments",
	"search",
	"bot_personas",
	"mute_archive",
	"threads",
	"branches",
	"reasoning_rows",
	"read_aloud",
	"call_buttons",
];

/// A room this host created, with everything the product declared about it.
struct RoomRecord {
	participating_as: ChatRoomParticipation,
	metadata: ChatRoomMetadata,
	/// The `https` icon of a first-version caller, or empty.
	icon_url: String,
	config: ChatRoomConfig,
	preview: Option<ChatRoomPreview>,
	footer: ChatRoomFooter,
	toolbar: Vec<ChatToolbarAction>,
	suggestions: Vec<ChatSuggestion>,
	commands: Vec<ChatCommandSpec>,
	activity: Option<ChatActivity>,
	/// In a room the host keeps, every message in the order it arrived. In a
	/// room the product keeps, the messages the screen saw: the page cache.
	messages: Vec<StoredMessage>,
	/// Participants the product resolved, by id.
	participants: BTreeMap<String, ChatParticipantEntry>,
	/// The newest message the user saw.
	last_seen: Option<String>,
	/// The optional features the product declared, kept whole, served or
	/// not.
	features: ChatRoomFeatures,
	/// The meta of each message, by message id.
	meta: BTreeMap<String, ChatMessageMeta>,
	/// The message ids of `meta` in the order their meta arrived, oldest
	/// first, so that a room the product keeps holds at most
	/// `CHAT_META_KEPT_MAX` of them.
	meta_order: std::collections::VecDeque<String>,
	/// The call of the room now.
	call: Option<ChatRoomCall>,
	/// Milliseconds since the Unix epoch when the mute the user set ends.
	muted_until: Option<u64>,
	/// The user archived the room.
	archived: bool,
}

impl RoomRecord {
	fn new(
		participating_as: ChatRoomParticipation,
		metadata: ChatRoomMetadata,
		icon_url: String,
		config: ChatRoomConfig,
		preview: Option<ChatRoomPreview>,
	) -> Self {
		Self {
			participating_as,
			metadata,
			icon_url,
			config,
			preview,
			footer: ChatRoomFooter::Input(Default::default()),
			toolbar: Vec::new(),
			suggestions: Vec::new(),
			commands: Vec::new(),
			activity: None,
			messages: Vec::new(),
			participants: BTreeMap::new(),
			last_seen: None,
			features: ChatRoomFeatures::default(),
			meta: BTreeMap::new(),
			meta_order: std::collections::VecDeque::new(),
			call: None,
			muted_until: None,
			archived: false,
		}
	}

	/// Keep the meta of one message. A room the product keeps holds the meta
	/// of at most `CHAT_META_KEPT_MAX` messages and drops the oldest.
	fn keep_meta(&mut self, message_id: &str, meta: ChatMessageMeta) {
		self.meta_order.retain(|kept| kept != message_id);
		self.meta_order.push_back(message_id.to_string());
		self.meta.insert(message_id.to_string(), meta);
		while self.meta_order.len() > CHAT_META_KEPT_MAX {
			if let Some(oldest) = self.meta_order.pop_front() {
				self.meta.remove(&oldest);
			}
		}
	}

	fn position(&self, message_id: &str) -> Option<usize> {
		self.messages.iter().position(|stored| stored.message.message_id == message_id)
	}

	/// The index of a message that is not deleted.
	fn live(&self, message_id: &str) -> Option<usize> {
		self.position(message_id).filter(|&index| !self.messages[index].deleted)
	}

	/// Whether the host keeps the messages of the room.
	fn host_store(&self) -> bool {
		!self.config.product_source
	}

	/// The message the chat list shows: the newest to arrive in a room the
	/// host keeps, the newest by time in the page cache of a room the
	/// product keeps. A reasoning row is never the list preview.
	fn last_message(&self) -> Option<&ChatMessage> {
		let reasoning = |stored: &&StoredMessage| {
			self.meta
				.get(&stored.message.message_id)
				.is_some_and(|meta| meta.reasoning.is_some())
		};
		let mut live = self
			.messages
			.iter()
			.filter(|stored| !stored.deleted)
			.filter(|stored| !reasoning(stored));
		if self.host_store() {
			live.next_back().map(|stored| &stored.message)
		} else {
			live.max_by_key(|stored| stored.message.timestamp).map(|stored| &stored.message)
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
			.filter(|stored| !stored.deleted && stored.message.origin.participant_id != USER)
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
	/// Deleted messages stay as a tombstone, so the screen can say so.
	deleted: bool,
	/// The content a first-version `post_message` sent. A `ChatMessage`
	/// cannot carry its URL media, so the surface draws it from here.
	legacy: Option<ChatMessageContent>,
}

impl StoredMessage {
	fn new(message: ChatMessage) -> Self {
		Self { message, reactions: Vec::new(), deleted: false, legacy: None }
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
			}),
		}
	}
	out
}

/// A bot the product registered. A bot is not a room, so registering one does
/// not republish the room list.
struct BotRecord {
	name: String,
	icon: String,
}

/// Rooms, bots and messages for one process.
#[derive(Default)]
struct State {
	/// Room id to what the product created and declared.
	rooms: BTreeMap<String, RoomRecord>,
	/// Bot id to what the product registered it as.
	bots: BTreeMap<String, BotRecord>,
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
	/// `Bot` objects in bot-id order.
	pub bots: Vec<Value>,
	/// `{ roomId, message }` for every message of a room the host keeps, in
	/// the order each arrived.
	pub messages: Vec<Value>,
}

/// A chat host that keeps everything in memory.
pub struct CliChatHost {
	state: Mutex<State>,
	transcript: Option<PathBuf>,
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

	/// Build a chat host recording to `transcript`. The file is truncated at
	/// startup so a run never reads an earlier run's messages as its own.
	pub fn new(transcript: Option<PathBuf>) -> Arc<Self> {
		if let Some(path) = transcript.as_ref() &&
			let Err(error) = std::fs::write(path, b"")
		{
			tracing::warn!(?path, %error, "chat transcript could not be truncated");
		}
		Arc::new(Self { state: Mutex::new(State::default()), transcript })
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
			bots: state.bots.iter().map(|(bot_id, bot)| bot_json(bot_id, bot)).collect(),
			messages: state
				.rooms
				.iter()
				.filter(|(_, room)| room.host_store())
				.flat_map(|(room_id, room)| {
					room.messages.iter().map(
						move |stored| json!({ "roomId": room_id, "message": stored_json(stored) }),
					)
				})
				.collect(),
		};
		let (sender, receiver) = mpsc::unbounded();
		state.observers.push(sender);
		(snapshot, receiver)
	}

	/// Play the user: mute `room_id` until `muted_until` and archive it or
	/// not. `u64::MAX` mutes the room with no end.
	pub fn set_user_state(
		&self,
		room_id: &str,
		muted_until: Option<u64>,
		archived: bool,
	) -> Result<(), String> {
		let mut state = self.lock();
		let room = state.rooms.get_mut(room_id).ok_or_else(|| unknown_room(room_id))?;
		room.muted_until = muted_until;
		room.archived = archived;
		let event = with_kind("room", room_json(room_id, room));
		Self::broadcast(&mut state, event);
		Ok(())
	}

	/// Show one notification the host delivered on every page, as a phone
	/// shows it in its notification list.
	pub fn notification_shown(&self, id: u32, text: &str, deeplink: Option<&str>) {
		let event = json!({ "kind": "notification", "id": id, "text": text, "deeplink": deeplink });
		Self::broadcast(&mut self.lock(), event);
	}

	/// The state of each room the user muted or archived, in room-id order.
	pub fn user_states(&self) -> Vec<ChatRoomUserState> {
		self.lock()
			.rooms
			.iter()
			.filter(|(_, room)| room.muted_until.is_some() || room.archived)
			.map(|(room_id, room)| ChatRoomUserState {
				room_id: room_id.clone(),
				muted_until: room.muted_until,
				archived: room.archived,
			})
			.collect()
	}

	/// The configuration the product declared for `room_id`, if the room
	/// exists.
	pub fn room_config(&self, room_id: &str) -> Option<ChatRoomConfig> {
		self.lock().rooms.get(room_id).map(|room| room.config.clone())
	}

	/// The call buttons `room_id` declares and the call it shows, if the
	/// room exists.
	pub fn room_call(
		&self,
		room_id: &str,
	) -> Option<(Option<ChatRoomCalls>, Option<ChatRoomCall>)> {
		self.lock()
			.rooms
			.get(room_id)
			.map(|room| (room.features.calls, room.call.clone()))
	}

	/// Whether `room_id` declared the command `name`. Only a declared name
	/// after `/` becomes a `Command` event; any other text is a message.
	pub fn declares_command(&self, room_id: &str, name: &str) -> bool {
		self.lock()
			.rooms
			.get(room_id)
			.is_some_and(|room| room.commands.iter().any(|command| command.name == name))
	}

	/// A message the room holds or the page cache saw, deleted or not.
	pub fn message(&self, room_id: &str, message_id: &str) -> Option<ChatMessage> {
		let state = self.lock();
		let room = state.rooms.get(room_id)?;
		room.position(message_id).map(|index| room.messages[index].message.clone())
	}

	/// The messages of a room the host keeps whose list text holds `text`,
	/// case and diacritics ignored, newest first: `page_size` of them after
	/// the first `skip`, and whether more follow.
	pub fn search_messages(
		&self,
		room_id: &str,
		text: &str,
		skip: usize,
		page_size: usize,
	) -> Result<(Vec<ChatMessage>, bool), String> {
		let state = self.lock();
		let room = state.rooms.get(room_id).ok_or_else(|| unknown_room(room_id))?;
		if !room.host_store() {
			return Err("the room keeps its messages in the product".to_string());
		}
		let needle = search_form(text);
		let mut matches = room
			.messages
			.iter()
			.rev()
			.filter(|stored| {
				!stored.deleted && search_form(&preview_text(&stored.message)).contains(&needle)
			})
			.skip(skip);
		let page: Vec<ChatMessage> =
			matches.by_ref().take(page_size).map(|stored| stored.message.clone()).collect();
		Ok((page, matches.next().is_some()))
	}

	/// A participant the product resolved for `room_id`.
	pub fn participant(&self, room_id: &str, participant_id: &str) -> Option<ChatParticipantEntry> {
		self.lock().rooms.get(room_id)?.participants.get(participant_id).cloned()
	}

	/// Keep the participants a product answered, so a later menu can name
	/// them.
	pub fn cache_participants(&self, room_id: &str, entries: &[ChatParticipantEntry]) {
		let mut state = self.lock();
		if let Some(room) = state.rooms.get_mut(room_id) {
			for entry in entries {
				room.participants.insert(entry.participant_id.clone(), entry.clone());
			}
		}
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
			edited_at: None,
			reactions: Vec::new(),
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

	/// Apply a change the user made in a room the host keeps: an edit or a
	/// delete of the user's own message, or a reaction to any message.
	pub fn change_as_user(&self, room_id: &str, change: UserChange) -> Result<(), String> {
		let mut state = self.lock();
		let room = state.rooms.get_mut(room_id).ok_or_else(|| unknown_room(room_id))?;
		if !room.host_store() {
			return Err("the room keeps its messages in the product".to_string());
		}
		let message_id = change.message_id().to_string();
		let index = room.live(&message_id).ok_or_else(|| unknown_message(&message_id))?;
		let stored = &mut room.messages[index];
		let own = stored.message.origin.participant_id == USER;
		match change {
			UserChange::Edit { body, .. } => {
				if !own {
					return Err("the user edits only the user's own messages".to_string());
				}
				stored.message.body = body;
				stored.message.edited_at = Some(now_millis());
			},
			UserChange::Delete { .. } => {
				// A failed message offers delete whoever wrote it.
				if !own && stored.message.status != ChatMessageStatus::Failed {
					return Err("the user deletes only the user's own messages".to_string());
				}
				stored.deleted = true;
			},
			UserChange::React { emoji, placed, .. } => stored.react(USER, &emoji, placed, true),
		}
		Self::announce_message(&mut state, room_id, index);
		Ok(())
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
				ChatUserAction::NewMessage { .. },
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
				room.live(message_id).inspect(|&index| {
					let message = &mut room.messages[index].message;
					message.body = body.clone();
					message.edited_at = Some(now_millis());
				})
			},
			(ChatUserAction::DeleteMessage { message_id }, _) => {
				room.live(message_id).inspect(|&index| room.messages[index].deleted = true)
			},
			(ChatUserAction::ReactionPlaced { message_id, emoji }, _) => room
				.live(message_id)
				.inspect(|&index| room.messages[index].react(USER, emoji, true, false)),
			(ChatUserAction::ReactionRemoved { message_id, emoji }, _) => room
				.live(message_id)
				.inspect(|&index| room.messages[index].react(USER, emoji, false, false)),
			(ChatUserAction::Seen { message_id }, _) => {
				room.last_seen = Some(message_id.clone());
				None
			},
			(ChatUserAction::Retry { .. } | ChatUserAction::NewMessage { .. }, _) => None,
		};
		if let Some(index) = changed {
			Self::announce_message(&mut state, room_id, index);
		}
	}

	/// The room screen closed: the activity line goes with it.
	pub fn close_room(&self, room_id: &str) {
		let mut state = self.lock();
		let Some(room) = state.rooms.get_mut(room_id) else {
			return;
		};
		if room.activity.take().is_some() {
			let event = with_kind("room", room_json(room_id, room));
			Self::broadcast(&mut state, event);
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
			"message": stored_json(&room.messages[index]),
			"list": list_json(room),
		});
		Self::broadcast(state, event);
	}

	/// Store one first-version message the product posted, and tell every
	/// observer, then append it to the transcript.
	fn accept_legacy(
		&self,
		room_id: &str,
		content: ChatMessageContent,
		alt: Option<String>,
	) -> Option<String> {
		let mut state = self.lock();
		if !state.rooms.contains_key(room_id) {
			return None;
		}
		let message_id = state.next_id(room_id);
		let line = json!({
			"kind": "message",
			"messageId": message_id,
			"roomId": room_id,
			"author": "product",
			"variant": variant_name(&content),
			// The payload as the host received it. A summary would let a
			// difference between what a product sent and what a host stored
			// hide behind the summary.
			"payload": hex::encode(content.encode()),
			"alt": alt,
		});
		let room = state.rooms.get_mut(room_id).expect("the room was just found");
		let changed = match &content {
			// A first-version reaction is a reaction by the product, not a
			// message of its own.
			ChatMessageContent::Reaction(reaction) |
			ChatMessageContent::ReactionRemoved(reaction) => {
				let placed = matches!(content, ChatMessageContent::Reaction(_));
				room.live(&reaction.message_id).inspect(|&index| {
					room.messages[index].react(PRODUCT, &reaction.emoji, placed, true);
				})
			},
			_ => {
				let mut stored = StoredMessage::new(ChatMessage {
					message_id: message_id.clone(),
					origin: ChatMessageOrigin { participant_id: PRODUCT.to_string() },
					timestamp: now_millis(),
					body: legacy_body(&content, alt.as_deref()),
					status: ChatMessageStatus::Sent,
					reply_to: None,
					edited_at: None,
					reactions: Vec::new(),
				});
				stored.legacy = Some(content);
				room.messages.push(stored);
				Some(room.messages.len() - 1)
			},
		};
		if let Some(index) = changed {
			Self::announce_message(&mut state, room_id, index);
		}
		drop(state);
		self.record(line);
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
		let Some(path) = self.transcript.as_ref() else {
			return;
		};
		let appended = OpenOptions::new()
			.create(true)
			.append(true)
			.open(path)
			.and_then(|mut file| writeln!(file, "{line}"));
		if let Err(error) = appended {
			tracing::warn!(?path, %error, "chat transcript could not be appended to");
		}
	}
}

/// A change the user makes to one message of a room the host keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserChange {
	Edit { message_id: String, body: ChatMessageBody },
	Delete { message_id: String },
	React { message_id: String, emoji: String, placed: bool },
}

impl UserChange {
	fn message_id(&self) -> &str {
		match self {
			Self::Edit { message_id, .. } |
			Self::Delete { message_id } |
			Self::React { message_id, .. } => message_id,
		}
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
		ChatFeedUpdate::Deleted { message_id } |
		ChatFeedUpdate::ReactionPlaced { message_id, .. } |
		ChatFeedUpdate::ReactionRemoved { message_id, .. } |
		ChatFeedUpdate::StatusChanged { message_id, .. } => match room.live(message_id) {
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
			stored.legacy = None;
		},
		ChatFeedUpdate::Deleted { message_id } => {
			stored.deleted = true;
			if persist {
				room.meta.remove(message_id);
				room.meta_order.retain(|kept| kept != message_id);
			}
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
		ChatFeedUpdate::Deleted { message_id } => ("Deleted", message_id),
		ChatFeedUpdate::ReactionPlaced { message_id, .. } => ("ReactionPlaced", message_id),
		ChatFeedUpdate::ReactionRemoved { message_id, .. } => ("ReactionRemoved", message_id),
		ChatFeedUpdate::StatusChanged { message_id, .. } => ("StatusChanged", message_id),
	}
}

/// The body a first-version message shows as. URL media cannot be a
/// `ChatMediaItem`, so it is left out, and a file keeps its text.
fn legacy_body(content: &ChatMessageContent, alt: Option<&str>) -> ChatMessageBody {
	match content {
		ChatMessageContent::Text { text } => ChatMessageBody::Text { text: text.clone() },
		ChatMessageContent::RichText(rich) => {
			ChatMessageBody::RichText { text: rich.text.clone(), media: Vec::new() }
		},
		ChatMessageContent::Actions(actions) => ChatMessageBody::Actions(actions.clone()),
		ChatMessageContent::File(file) => {
			ChatMessageBody::RichText { text: file.text.clone(), media: Vec::new() }
		},
		ChatMessageContent::Custom(custom) => ChatMessageBody::Custom {
			message_type: custom.message_type.clone(),
			payload: custom.payload.clone(),
			preview_text: alt.map(str::to_string),
		},
		ChatMessageContent::Reaction(_) | ChatMessageContent::ReactionRemoved(_) => {
			ChatMessageBody::Text { text: String::new() }
		},
	}
}

/// `body` with the surface-protocol `kind` discriminant added.
fn with_kind(kind: &str, mut body: Value) -> Value {
	body["kind"] = Value::from(kind);
	body
}

/// The form search compares: lower case, without diacritics.
fn search_form(text: &str) -> String {
	use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
	text.nfd()
		.filter(|character| !is_combining_mark(*character))
		.collect::<String>()
		.to_lowercase()
}

/// The chat-list text of one message: its text, its `preview_text`, the
/// `alt` of its first media item, or a generic line.
pub fn preview_text(message: &ChatMessage) -> String {
	let text = match &message.body {
		ChatMessageBody::Text { text } => Some(text.clone()),
		ChatMessageBody::RichText { text, media } => text
			.clone()
			.or_else(|| media.first().and_then(|item| media_meta(item).alt.clone())),
		ChatMessageBody::Actions(actions) => actions.text.clone(),
		ChatMessageBody::Custom { preview_text, .. } => preview_text.clone(),
	};
	text.filter(|text| !text.trim().is_empty())
		.unwrap_or_else(|| SENT_A_MESSAGE.to_string())
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
		"avatar": room.metadata.avatar.as_ref().map(resource_json),
		"icon": room.icon_url,
		"config": config_json(&room.config),
		"footer": footer_json(&room.footer),
		"toolbar": room.toolbar.iter().map(|action| json!({
			"actionId": action.action_id,
			"label": action.label,
			"icon": resource_json(&action.icon),
			"handler": handler_json(&action.handler),
		})).collect::<Vec<_>>(),
		"suggestions": room.suggestions.iter().map(|suggestion| json!({
			"suggestionId": suggestion.suggestion_id,
			"text": suggestion.text,
			"sendsText": suggestion.sends_text,
		})).collect::<Vec<_>>(),
		"commands": room.commands.iter().map(|command| json!({
			"name": command.name,
			"description": command.description,
			"argumentHint": command.argument_hint,
		})).collect::<Vec<_>>(),
		"activity": room.activity.as_ref().map(|activity| json!({
			"text": activity.text,
			"participantId": activity.participant_id,
		})),
		"lastSeen": room.last_seen,
		"list": list_json(room),
		"features": features_json(&room.features),
		"meta": room.meta.iter().map(|(message_id, meta)| (message_id.clone(), meta_json(meta))).collect::<serde_json::Map<_, _>>(),
		"call": room.call.as_ref().map(call_json),
		"userState": { "mutedUntil": room.muted_until, "archived": room.archived },
	})
}

/// The surface view of the optional features of a room.
fn features_json(features: &ChatRoomFeatures) -> Value {
	json!({
		"preimageAttachments": features.preimage_attachments,
		"search": features.search,
		"calls": features.calls.map(|calls| json!({ "audio": calls.audio, "video": calls.video })),
		"threadOf": features.thread_of.as_ref().map(|parent| json!({
			"roomId": parent.room_id,
			"messageId": parent.message_id,
		})),
		"readAloud": features.read_aloud,
	})
}

/// The surface view of the meta of one message.
fn meta_json(meta: &ChatMessageMeta) -> Value {
	json!({
		"thread": meta.thread.as_ref().map(|thread| json!({
			"roomId": thread.room_id,
			"replyCount": thread.reply_count,
			"lastReplyAt": thread.last_reply_at,
			"participantIds": thread.participant_ids,
		})),
		"branch": meta.branch.map(|branch| json!({ "index": branch.index, "count": branch.count })),
		"reasoning": meta.reasoning.map(|reasoning| json!({ "durationMs": reasoning.duration_ms })),
	})
}

/// The surface view of the call of a room.
fn call_json(call: &ChatRoomCall) -> Value {
	match call {
		ChatRoomCall::Ringing { call_id, video, caller } => json!({
			"type": "Ringing",
			"callId": call_id,
			"video": video,
			"caller": caller,
		}),
		ChatRoomCall::Active { call_id, video, started_at, media_session } => json!({
			"type": "Active",
			"callId": call_id,
			"video": video,
			"startedAt": started_at,
			"mediaSession": media_session.map(|session| format!("0x{}", hex::encode(session))),
		}),
	}
}

/// The chat-list row of a room: the product's preview where it set one, the
/// host default of each field where it did not.
fn list_json(room: &RoomRecord) -> Value {
	let preview = room.preview.as_ref();
	let last = room.last_message();
	let text = match preview.and_then(|preview| preview.text.as_ref()) {
		Some(text) => json!({ "text": text.text, "ongoing": text.ongoing }),
		None => last.map_or(
			Value::Null,
			|message| json!({ "text": preview_text(message), "ongoing": false }),
		),
	};
	let badge = match preview.and_then(|preview| preview.badge.as_ref()) {
		Some(ChatRoomBadge::UnreadCount { count }) if *count > 0 => {
			json!({ "type": "UnreadCount", "count": count })
		},
		Some(ChatRoomBadge::Image { resource }) => {
			json!({ "type": "Image", "resource": resource_json(resource) })
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
		"pinned": preview.is_some_and(|preview| preview.order == ChatRoomOrder::PinToTop),
		"timestamp": preview
			.and_then(|preview| preview.timestamp)
			.or_else(|| last.map(|message| message.timestamp)),
		"text": text,
		"badge": badge,
		"previewDeclared": preview.is_some(),
	})
}

fn config_json(config: &ChatRoomConfig) -> Value {
	json!({
		"productSource": config.product_source,
		"reactionDetails": config.reaction_details,
		"participants": config.participants,
		"messageActions": config.message_actions,
		"participantActions": config.participant_actions,
		"allowedReactions": config.allowed_reactions,
		"placeholder": config.placeholder,
		"reportSeen": config.report_seen,
	})
}

fn footer_json(footer: &ChatRoomFooter) -> Value {
	match footer {
		ChatRoomFooter::Input(input) => json!({
			"type": "Input",
			"placeholder": input.placeholder,
			"photo": input.photo,
			"video": input.video,
			"file": input.file,
			"payment": input.payment.as_ref().map(|payment| match payment {
				truapi::latest::ChatInputPayment::Enabled { destination } => {
					json!({ "destination": format!("0x{}", hex::encode(destination)) })
				},
			}),
		}),
		ChatRoomFooter::Empty => json!({ "type": "Empty" }),
		ChatRoomFooter::Custom => json!({ "type": "Custom" }),
	}
}

/// The surface view of a menu or toolbar handler.
pub fn handler_json(handler: &ChatActionHandler) -> Value {
	match handler {
		ChatActionHandler::Deeplink { url } => json!({ "type": "Deeplink", "url": url }),
		ChatActionHandler::Callback => json!({ "type": "Callback" }),
	}
}

/// The surface view of a resource. Inline bytes travel as a `data:` URL the
/// page can draw; a preimage travels as its key, which the page shows.
pub fn resource_json(resource: &ChatResource) -> Value {
	match resource {
		ChatResource::Inline { mime_type, bytes } => json!({
			"type": "Inline",
			"mimeType": mime_type,
			"size": bytes.len(),
			"dataUrl": format!("data:{mime_type};base64,{}", BASE64.encode(bytes)),
		}),
		ChatResource::Preimage { key } => json!({
			"type": "Preimage",
			"key": format!("0x{}", hex::encode(key)),
		}),
	}
}

fn stored_json(stored: &StoredMessage) -> Value {
	let mut value = message_json(&stored.message);
	value["deleted"] = Value::from(stored.deleted);
	if let Some(legacy) = &stored.legacy {
		value["legacy"] = content_json(legacy);
	}
	value
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

/// The surface view of a message body. Every field is kept, and a custom
/// payload travels as hex, so the surface draws what the host holds.
pub fn body_json(body: &ChatMessageBody) -> Value {
	match body {
		ChatMessageBody::Text { text } => json!({ "type": "Text", "text": text }),
		ChatMessageBody::RichText { text, media } => json!({
			"type": "RichText",
			"text": text,
			"media": media.iter().map(media_json).collect::<Vec<_>>(),
		}),
		ChatMessageBody::Actions(actions) => json!({
			"type": "Actions",
			"text": actions.text,
			"actions": actions
				.actions
				.iter()
				.map(|action| json!({ "actionId": action.action_id, "title": action.title }))
				.collect::<Vec<_>>(),
			"layout": layout_name(actions.layout),
		}),
		ChatMessageBody::Custom { message_type, payload, preview_text } => json!({
			"type": "Custom",
			"messageType": message_type,
			"payloadHex": hex::encode(payload),
			"previewText": preview_text,
		}),
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

fn layout_name(layout: ChatActionLayout) -> &'static str {
	match layout {
		ChatActionLayout::Column => "Column",
		ChatActionLayout::Grid => "Grid",
	}
}

fn bot_json(bot_id: &str, bot: &BotRecord) -> Value {
	json!({ "botId": bot_id, "name": bot.name, "icon": bot.icon })
}

/// The surface-protocol view of first-version content. Every field is kept,
/// and a custom payload travels as hex, so the surface draws what the host
/// holds.
pub fn content_json(content: &ChatMessageContent) -> Value {
	match content {
		ChatMessageContent::Text { text } => json!({ "type": "Text", "text": text }),
		ChatMessageContent::RichText(rich) => json!({
			"type": "RichText",
			"text": rich.text,
			"media": rich.media.iter().map(|media| json!({ "url": media.url })).collect::<Vec<_>>(),
		}),
		ChatMessageContent::Actions(actions) => json!({
			"type": "Actions",
			"text": actions.text,
			"actions": actions
				.actions
				.iter()
				.map(|action| json!({ "actionId": action.action_id, "title": action.title }))
				.collect::<Vec<_>>(),
			"layout": layout_name(actions.layout),
		}),
		ChatMessageContent::File(file) => json!({
			"type": "File",
			"url": file.url,
			"fileName": file.file_name,
			"mimeType": file.mime_type,
			"sizeBytes": file.size_bytes,
			"text": file.text,
		}),
		ChatMessageContent::Reaction(reaction) => json!({
			"type": "Reaction",
			"messageId": reaction.message_id,
			"emoji": reaction.emoji,
		}),
		ChatMessageContent::ReactionRemoved(reaction) => json!({
			"type": "ReactionRemoved",
			"messageId": reaction.message_id,
			"emoji": reaction.emoji,
		}),
		ChatMessageContent::Custom(custom) => json!({
			"type": "Custom",
			"messageType": custom.message_type,
			"payloadHex": hex::encode(&custom.payload),
		}),
	}
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
		ChatMessageBody::Actions(_) => "Actions",
		ChatMessageBody::Custom { .. } => "Custom",
	}
}

#[async_trait]
impl ChatPlatform for CliChatHost {
	async fn create_chat_room(
		&self,
		_product: &ProductContext,
		request: HostChatCreateRoomRequest,
		icon_url: String,
	) -> Result<HostChatCreateRoomResponse, HostChatCreateRoomError> {
		let mut state = self.lock();
		// A room that exists keeps what it declared.
		let status = if state.rooms.contains_key(&request.room_id) {
			ChatRoomRegistrationStatus::Exists
		} else {
			let room = RoomRecord::new(
				ChatRoomParticipation::RoomHost,
				request.metadata.clone(),
				icon_url.clone(),
				request.config.clone(),
				request.preview.clone(),
			);
			let event = with_kind("room", room_json(&request.room_id, &room));
			state.rooms.insert(request.room_id.clone(), room);
			Self::republish(&mut state);
			Self::broadcast(&mut state, event);
			ChatRoomRegistrationStatus::New
		};
		drop(state);
		self.record_registration("room", &request.room_id, &request.metadata.name, &icon_url);
		Ok(HostChatCreateRoomResponse { status })
	}

	async fn register_chat_bot(
		&self,
		_product: &ProductContext,
		request: HostChatRegisterBotRequest,
	) -> Result<HostChatRegisterBotResponse, HostChatRegisterBotError> {
		let mut state = self.lock();
		// A repeat keeps the persona and replaces its name and icon.
		let bot = BotRecord { name: request.name.clone(), icon: request.icon.clone() };
		let event = with_kind("bot", bot_json(&request.bot_id, &bot));
		let status = match state.bots.insert(request.bot_id.clone(), bot) {
			Some(_) => ChatBotRegistrationStatus::Exists,
			None => ChatBotRegistrationStatus::New,
		};
		Self::broadcast(&mut state, event);
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
		let message_id =
			self.accept_legacy(&request.room_id, request.payload, request.alt).ok_or_else(
				|| HostChatPostMessageError::Unknown { reason: unknown_room(&request.room_id) },
			)?;
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

	async fn set_chat_room_config(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomConfigRequest,
	) -> Result<(), GenericError> {
		self.declare("config", &request.room_id, &request, |room| {
			// A change of mode discards what the room held: the host's
			// messages or the page cache, which belong to the other mode.
			if room.config.product_source != request.config.product_source {
				room.messages.clear();
				room.participants.clear();
				room.last_seen = None;
			}
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
		// Idempotent: a room that is gone is deleted. Its thread rooms, which
		// no list shows, go with it.
		if state.rooms.remove(&request.room_id).is_some() {
			let threads: Vec<String> = state
				.rooms
				.iter()
				.filter(|(_, room)| {
					room.features
						.thread_of
						.as_ref()
						.is_some_and(|parent| parent.room_id == request.room_id)
				})
				.map(|(room_id, _)| room_id.clone())
				.collect();
			for thread in &threads {
				state.rooms.remove(thread);
			}
			Self::republish(&mut state);
			for room_id in std::iter::once(&request.room_id).chain(&threads) {
				Self::broadcast(&mut state, json!({ "kind": "roomDeleted", "roomId": room_id }));
			}
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

	async fn set_chat_room_toolbar(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomToolbarRequest,
	) -> Result<(), GenericError> {
		self.declare("toolbar", &request.room_id, &request, |room| {
			room.toolbar = request.actions.clone();
		})
	}

	async fn set_chat_room_suggestions(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomSuggestionsRequest,
	) -> Result<(), GenericError> {
		self.declare("suggestions", &request.room_id, &request, |room| {
			room.suggestions = request.suggestions.clone();
		})
	}

	async fn set_chat_room_commands(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomCommandsRequest,
	) -> Result<(), GenericError> {
		self.declare("commands", &request.room_id, &request, |room| {
			room.commands = request.commands.clone();
		})
	}

	async fn set_chat_room_activity(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomActivityRequest,
	) -> Result<(), GenericError> {
		self.declare("activity", &request.room_id, &request, |room| {
			room.activity = request.activity.clone();
		})
	}

	async fn chat_capabilities(
		&self,
		_product: &ProductContext,
	) -> Result<ChatFeatureNames, GenericError> {
		Ok(ChatFeatureNames { names: CLI_CHAT_FEATURES.map(str::to_string).to_vec() })
	}

	async fn read_chat_room_features(
		&self,
		_product: &ProductContext,
		room_id: String,
	) -> Result<ChatRoomFeatures, GenericError> {
		Ok(self
			.lock()
			.rooms
			.get(&room_id)
			.map(|room| room.features.clone())
			.unwrap_or_default())
	}

	async fn set_chat_room_features(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomFeaturesRequest,
	) -> Result<(), GenericError> {
		self.declare("features", &request.room_id, &request, |room| {
			room.features = request.features.clone();
		})
	}

	async fn set_chat_message_meta(
		&self,
		_product: &ProductContext,
		request: HostChatSetMessageMetaRequest,
	) -> Result<(), HostChatUpdateFeedError> {
		let mut state = self.lock();
		let room = state
			.rooms
			.get_mut(&request.room_id)
			.ok_or(HostChatUpdateFeedError::UnknownRoom)?;
		// A room the host keeps holds meta only for a message it holds. A room
		// the product keeps holds the meta of a message the screen did not see
		// yet, and applies it when the page arrives.
		if room.host_store() && room.live(&request.message_id).is_none() {
			return Err(HostChatUpdateFeedError::UnknownMessage);
		}
		room.keep_meta(&request.message_id, request.meta.clone());
		let event = with_kind("room", room_json(&request.room_id, room));
		Self::broadcast(&mut state, event);
		drop(state);
		self.record_declaration("meta", &request.room_id, &request);
		Ok(())
	}

	async fn set_chat_room_call(
		&self,
		_product: &ProductContext,
		request: HostChatSetRoomCallRequest,
	) -> Result<(), GenericError> {
		self.declare("call", &request.room_id, &request, |room| {
			room.call = request.call.clone();
		})
	}

	async fn read_chat_room_user_states(
		&self,
		_product: &ProductContext,
	) -> Result<ChatRoomUserStates, GenericError> {
		Ok(ChatRoomUserStates { rooms: self.user_states() })
	}

	async fn read_chat_bots(&self, _product: &ProductContext) -> Result<ChatBotIds, GenericError> {
		Ok(ChatBotIds { bot_ids: self.lock().bots.keys().cloned().collect() })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::Decode;
	use std::fs::read_to_string;
	use truapi::latest::{
		ChatActivity, ChatInputConfig, ChatRoomPreviewText, HostChatSetRoomFooterRequest,
	};

	fn product() -> ProductContext {
		ProductContext::new("chat.dot".to_string()).expect("valid product id")
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
			metadata: ChatRoomMetadata {
				name: "Support".to_string(),
				avatar: None,
				description: None,
			},
			config: ChatRoomConfig::default(),
			preview: None,
		}
	}

	fn source_config() -> ChatRoomConfig {
		ChatRoomConfig { product_source: true, report_seen: true, ..ChatRoomConfig::default() }
	}

	fn create(host: &CliChatHost, request: HostChatCreateRoomRequest) {
		futures::executor::block_on(host.create_chat_room(&product(), request, String::new()))
			.expect("a room is created");
	}

	/// A host with the room `support` the host keeps and the room `kept`
	/// the product keeps.
	fn host_with_rooms(transcript: Option<PathBuf>) -> Arc<CliChatHost> {
		let host = CliChatHost::new(transcript);
		create(&host, room("support"));
		create(&host, HostChatCreateRoomRequest { config: source_config(), ..room("kept") });
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
			edited_at: None,
			reactions: Vec::new(),
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
		// The surface draws it as a message of the product.
		let (snapshot, _) = host.observe();
		assert_eq!(snapshot.messages[0]["message"]["origin"], PRODUCT);
		assert_eq!(snapshot.messages[0]["message"]["body"]["text"], "line one\nline two");
	}

	#[test]
	fn a_first_version_reaction_is_a_reaction_of_the_product() {
		use truapi::latest::ChatReaction;
		let host = CliChatHost::new(None);
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
		let target = post(text("rate me"));
		post(ChatMessageContent::Reaction(ChatReaction {
			message_id: target.clone(),
			emoji: "+1".to_string(),
		}));

		let messages = host.observe().0.messages;
		assert_eq!(messages.len(), 1, "a reaction is not a message");
		assert_eq!(
			messages[0]["message"]["reactions"],
			json!([{ "emoji": "+1", "count": 1, "reactedByUser": false }])
		);
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

		let origins: Vec<_> = host
			.observe()
			.0
			.messages
			.iter()
			.map(|entry| entry["message"]["origin"].clone())
			.collect();
		assert_eq!(origins, vec![PRODUCT, USER, PRODUCT]);
		let recorded = transcript_lines(transcript.path());
		assert_eq!(recorded[2]["author"], "person");
		assert_eq!(recorded[2]["messageId"], "m2");
		assert_eq!(recorded[2]["payload"], hex::encode(body("hello").encode()));

		assert!(host.post_user_message("elsewhere", body("lost"), None).is_err());
		assert_eq!(host.observe().0.messages.len(), 3);
	}

	#[test]
	fn a_room_the_product_keeps_takes_no_message_from_the_user_through_the_host() {
		let host = host_with_rooms(None);
		assert!(host.post_user_message("kept", body("hi"), None).is_err());
		let change = UserChange::Delete { message_id: "x".to_string() };
		assert!(host.change_as_user("kept", change).is_err());
	}

	#[test]
	fn a_room_that_exists_keeps_what_it_declared() {
		let host = CliChatHost::new(None);
		create(&host, HostChatCreateRoomRequest { config: source_config(), ..room("kept") });

		let again = futures::executor::block_on(host.create_chat_room(
			&product(),
			HostChatCreateRoomRequest {
				metadata: ChatRoomMetadata {
					name: "Renamed".to_string(),
					avatar: None,
					description: None,
				},
				..room("kept")
			},
			String::new(),
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
	fn a_new_configuration_replaces_the_old_and_a_new_mode_drops_the_messages() {
		let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
		let host = host_with_rooms(Some(transcript.path().to_path_buf()));
		host.post_user_message("support", body("before"), None)
			.expect("the room exists");
		let (_, mut events) = host.observe();

		let request = HostChatSetRoomConfigRequest {
			room_id: "support".to_string(),
			config: ChatRoomConfig {
				placeholder: Some("Nothing here".to_string()),
				allowed_reactions: Some(vec!["+1".to_string()]),
				..ChatRoomConfig::default()
			},
		};
		futures::executor::block_on(host.set_chat_room_config(&product(), request.clone()))
			.expect("the room exists");
		let event = events.try_recv().expect("the room event");
		assert_eq!(event["config"]["placeholder"], "Nothing here");
		assert_eq!(event["config"]["allowedReactions"], json!(["+1"]));
		assert_eq!(host.observe().0.messages.len(), 1, "the same mode keeps the messages");
		let line = transcript_lines(transcript.path()).pop().expect("the config line");
		assert_eq!(
			line,
			json!({
				"kind": "config",
				"roomId": "support",
				"payload": hex::encode(request.encode()),
			})
		);

		futures::executor::block_on(host.set_chat_room_config(
			&product(),
			HostChatSetRoomConfigRequest {
				room_id: "support".to_string(),
				config: source_config(),
			},
		))
		.expect("the room exists");
		assert!(host.observe().0.messages.is_empty(), "a new mode drops what the host held");
		assert_eq!(room_view(&host, "support")["config"]["productSource"], true);

		let missing = futures::executor::block_on(host.set_chat_room_config(
			&product(),
			HostChatSetRoomConfigRequest { room_id: "gone".to_string(), config: source_config() },
		));
		assert!(missing.is_err());
	}

	#[test]
	fn metadata_replaces_the_name_avatar_and_description() {
		let host = host_with_rooms(None);
		let (_, mut events) = host.observe();
		futures::executor::block_on(host.set_chat_room_metadata(
			&product(),
			HostChatSetRoomMetadataRequest {
				room_id: "support".to_string(),
				metadata: ChatRoomMetadata {
					name: "Help desk".to_string(),
					avatar: Some(ChatResource::Inline {
						mime_type: "image/png".to_string(),
						bytes: vec![1, 2, 3],
					}),
					description: Some("We answer".to_string()),
				},
			},
		))
		.expect("the room exists");

		let event = events.try_recv().expect("the room event");
		assert_eq!(event["name"], "Help desk");
		assert_eq!(event["description"], "We answer");
		assert_eq!(
			event["avatar"],
			json!({
				"type": "Inline",
				"mimeType": "image/png",
				"size": 3,
				"dataUrl": "data:image/png;base64,AQID",
			})
		);
	}

	#[test]
	fn the_list_row_takes_the_preview_and_defaults_the_rest() {
		let host = host_with_rooms(None);
		futures::executor::block_on(host.post_chat_message(
			&product(),
			HostChatPostMessageRequest {
				room_id: "support".to_string(),
				payload: text("newest"),
				alt: None,
			},
		))
		.expect("the room exists");
		// With nothing declared: the newest text and the host's unread count.
		let list = &room_view(&host, "support")["list"];
		assert_eq!(list["pinned"], false);
		assert_eq!(list["text"], json!({ "text": "newest", "ongoing": false }));
		assert_eq!(list["badge"], json!({ "type": "UnreadCount", "count": 1 }));

		let preview = |preview: ChatRoomPreview| {
			futures::executor::block_on(host.set_chat_room_preview(
				&product(),
				HostChatSetRoomPreviewRequest { room_id: "support".to_string(), preview },
			))
			.expect("the room exists");
			room_view(&host, "support")["list"].clone()
		};
		let list = preview(ChatRoomPreview {
			order: ChatRoomOrder::PinToTop,
			timestamp: Some(42),
			badge: Some(ChatRoomBadge::UnreadCount { count: 7 }),
			text: Some(ChatRoomPreviewText { text: "Bot is typing".to_string(), ongoing: true }),
		});
		assert_eq!(list["pinned"], true);
		assert_eq!(list["timestamp"], 42);
		assert_eq!(list["badge"], json!({ "type": "UnreadCount", "count": 7 }));
		assert_eq!(list["text"], json!({ "text": "Bot is typing", "ongoing": true }));

		// The variant that hides the badge, by its wire index rather than
		// its name.
		let hidden = ChatRoomBadge::decode(&mut &[0u8][..]).expect("variant 0 hides the badge");
		let list = preview(ChatRoomPreview {
			order: ChatRoomOrder::Timestamp,
			timestamp: None,
			badge: Some(hidden),
			text: None,
		});
		assert_eq!(list["pinned"], false);
		assert_eq!(list["badge"], Value::Null);
		assert_eq!(list["text"]["text"], "newest");
		let list = preview(ChatRoomPreview {
			order: ChatRoomOrder::Timestamp,
			timestamp: None,
			badge: Some(ChatRoomBadge::UnreadCount { count: 0 }),
			text: None,
		});
		assert_eq!(list["badge"], Value::Null, "a count of 0 shows nothing");
		let list = preview(ChatRoomPreview {
			order: ChatRoomOrder::Timestamp,
			timestamp: None,
			badge: Some(ChatRoomBadge::Image {
				resource: ChatResource::Preimage { key: vec![7; 32] },
			}),
			text: None,
		});
		assert_eq!(list["badge"]["type"], "Image");
		assert_eq!(list["badge"]["resource"]["key"], format!("0x{}", "07".repeat(32)));
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
		assert_eq!(kinds, ["room", "room", "deleteRoom", "deleteRoom"]);
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

		feed(&host, "support", ChatFeedUpdate::Deleted { message_id: "p-1".to_string() }, true)
			.expect("any message is deleted");
		let deleted = drain(&mut events).pop().expect("the delete event");
		assert_eq!(deleted["message"]["deleted"], true);
		assert_eq!(deleted["list"]["text"]["text"], "mine", "the preview skips a deleted message");
		assert_eq!(
			feed(&host, "support", ChatFeedUpdate::Deleted { message_id: "p-1".to_string() }, true),
			Err(HostChatUpdateFeedError::UnknownMessage)
		);
		assert_eq!(
			feed(&host, "gone", ChatFeedUpdate::Deleted { message_id: "p-1".to_string() }, true),
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
		feed(&host, "kept", ChatFeedUpdate::Deleted { message_id: "old".to_string() }, false)
			.expect("an update for a message off screen is fine");
		assert!(drain(&mut events).is_empty());

		feed(&host, "kept", ChatFeedUpdate::New(message("s-1", PRODUCT, "hi")), false)
			.expect("a new message goes to the screen");
		let mut summary = message("s-2", USER, "with reactions");
		summary.reactions =
			vec![ChatReactionSummary { emoji: "+1".to_string(), count: 3, reacted_by_user: false }];
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
	fn toolbar_suggestions_commands_activity_and_footer_are_kept() {
		let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
		let host = host_with_rooms(Some(transcript.path().to_path_buf()));
		let icon = ChatResource::Inline { mime_type: "image/png".to_string(), bytes: vec![9] };
		futures::executor::block_on(host.set_chat_room_toolbar(
			&product(),
			HostChatSetRoomToolbarRequest {
				room_id: "support".to_string(),
				actions: vec![ChatToolbarAction {
					action_id: "tb-1".to_string(),
					label: "Open".to_string(),
					icon,
					handler: ChatActionHandler::Deeplink { url: "polkadot://x".to_string() },
				}],
			},
		))
		.expect("the room exists");
		futures::executor::block_on(host.set_chat_room_suggestions(
			&product(),
			HostChatSetRoomSuggestionsRequest {
				room_id: "support".to_string(),
				suggestions: vec![ChatSuggestion {
					suggestion_id: "sg".to_string(),
					text: "Say hello".to_string(),
					sends_text: true,
				}],
			},
		))
		.expect("the room exists");
		futures::executor::block_on(host.set_chat_room_commands(
			&product(),
			HostChatSetRoomCommandsRequest {
				room_id: "support".to_string(),
				commands: vec![ChatCommandSpec {
					name: "help".to_string(),
					description: "List commands".to_string(),
					argument_hint: None,
				}],
			},
		))
		.expect("the room exists");
		futures::executor::block_on(host.set_chat_room_activity(
			&product(),
			HostChatSetRoomActivityRequest {
				room_id: "support".to_string(),
				activity: Some(ChatActivity { text: "Typing".to_string(), participant_id: None }),
			},
		))
		.expect("the room exists");
		futures::executor::block_on(host.set_chat_room_footer(
			&product(),
			HostChatSetRoomFooterRequest {
				room_id: "support".to_string(),
				footer: ChatRoomFooter::Input(ChatInputConfig {
					placeholder: Some("Ask".to_string()),
					photo: false,
					video: false,
					file: true,
					payment: Some(truapi::latest::ChatInputPayment::Enabled {
						destination: [5; 32],
					}),
				}),
			},
		))
		.expect("the room exists");

		let view = room_view(&host, "support");
		assert_eq!(view["toolbar"][0]["actionId"], "tb-1");
		assert_eq!(
			view["toolbar"][0]["handler"],
			json!({ "type": "Deeplink", "url": "polkadot://x" })
		);
		assert_eq!(view["toolbar"][0]["icon"]["dataUrl"], "data:image/png;base64,CQ==");
		assert_eq!(
			view["suggestions"],
			json!([{ "suggestionId": "sg", "text": "Say hello", "sendsText": true }])
		);
		assert_eq!(view["commands"][0]["name"], "help");
		assert_eq!(view["activity"]["text"], "Typing");
		assert_eq!(view["footer"]["type"], "Input");
		assert_eq!(view["footer"]["placeholder"], "Ask");
		assert_eq!(view["footer"]["file"], true);
		assert_eq!(view["footer"]["payment"]["destination"], format!("0x{}", "05".repeat(32)));
		assert!(host.declares_command("support", "help"));
		assert!(!host.declares_command("support", "nope"));
		assert!(!host.declares_command("kept", "help"));
		// The default footer of a room that declared none.
		assert_eq!(
			room_view(&host, "kept")["footer"],
			json!({
				"type": "Input", "placeholder": null, "photo": true, "video": true, "file": false, "payment": null,
			})
		);

		// The activity line goes when the screen closes.
		host.close_room("support");
		assert_eq!(room_view(&host, "support")["activity"], Value::Null);

		let kinds: Vec<_> = transcript_lines(transcript.path())
			.into_iter()
			.map(|line| line["kind"].as_str().unwrap_or_default().to_string())
			.filter(|kind| kind != "room")
			.collect();
		assert_eq!(kinds, ["toolbar", "suggestions", "commands", "activity", "footer"]);
	}

	#[test]
	fn the_user_edits_and_deletes_only_the_users_own_messages() {
		let host = host_with_rooms(None);
		let mine = host.post_user_message("support", body("mine"), None).expect("the room exists");
		feed(&host, "support", ChatFeedUpdate::New(message("p-1", PRODUCT, "theirs")), true)
			.expect("stored");
		let edit = |message_id: &str| UserChange::Edit {
			message_id: message_id.to_string(),
			body: body("changed"),
		};

		host.change_as_user("support", edit(&mine.message_id))
			.expect("an own message is edited");
		assert!(host.change_as_user("support", edit("p-1")).is_err());
		assert!(
			host.change_as_user("support", UserChange::Delete { message_id: "p-1".to_string() })
				.is_err()
		);
		host.change_as_user(
			"support",
			UserChange::React {
				message_id: "p-1".to_string(),
				emoji: "+1".to_string(),
				placed: true,
			},
		)
		.expect("any message takes a reaction");
		feed(
			&host,
			"support",
			ChatFeedUpdate::StatusChanged {
				message_id: "p-1".to_string(),
				status: ChatMessageStatus::Failed,
			},
			true,
		)
		.expect("stored");
		host.change_as_user("support", UserChange::Delete { message_id: "p-1".to_string() })
			.expect("a failed message offers delete");

		let edited = host.message("support", &mine.message_id).expect("kept");
		assert_eq!(edited.body, body("changed"));
		assert!(edited.edited_at.is_some());
		let reacted = host.message("support", "p-1").expect("a tombstone stays");
		assert!(reacted.reactions[0].reacted_by_user);
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
			&ChatUserAction::NewMessage { body: body("hello"), reply_to: None },
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
		host.apply_source_action(
			"kept",
			&ChatUserAction::DeleteMessage { message_id: "s-2".to_string() },
			&acknowledged,
		);
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
		assert_eq!(messages[3]["deleted"], true);
		assert_eq!(events.len(), 4, "a seen report changes no message");
		assert_eq!(room_view(&host, "kept")["lastSeen"], "s-1");
	}

	#[test]
	fn the_participants_a_product_named_are_kept() {
		use truapi::latest::ChatParticipant;
		let host = host_with_rooms(None);
		let entry = ChatParticipantEntry {
			participant_id: "p1".to_string(),
			participant: ChatParticipant::Person { person_id: 1 },
		};
		host.cache_participants("kept", std::slice::from_ref(&entry));
		assert_eq!(host.participant("kept", "p1"), Some(entry));
		assert_eq!(host.participant("kept", "p2"), None);
		assert_eq!(host.participant("support", "p1"), None);
	}

	#[test]
	fn the_list_text_of_a_message_falls_back_in_order() {
		use truapi::latest::{ChatAction, ChatActions};
		let with = |body| ChatMessage { body, ..message("m", PRODUCT, "") };
		let meta = |alt: Option<&str>| ChatMediaMeta {
			mime_type: "image/png".to_string(),
			size_bytes: None,
			width: None,
			height: None,
			placeholder: None,
			alt: alt.map(str::to_string),
		};
		let image = |alt| ChatMediaItem::Image {
			resource: ChatResource::Preimage { key: vec![0; 32] },
			meta: meta(alt),
		};
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
			preview_text(&with(ChatMessageBody::Actions(ChatActions {
				text: Some("Pick".to_string()),
				actions: vec![ChatAction { action_id: "a".to_string(), title: "A".to_string() }],
				layout: ChatActionLayout::Grid,
			}))),
			"Pick"
		);
		assert_eq!(
			preview_text(&with(ChatMessageBody::Custom {
				message_type: "card".to_string(),
				payload: vec![1],
				preview_text: Some("Sample card".to_string()),
			})),
			"Sample card"
		);
	}

	#[test]
	fn a_body_is_shown_with_every_field_the_product_sent() {
		let media = vec![
			ChatMediaItem::Image {
				resource: ChatResource::Inline {
					mime_type: "image/png".to_string(),
					bytes: vec![0xff],
				},
				meta: ChatMediaMeta {
					mime_type: "image/png".to_string(),
					size_bytes: Some(1),
					width: Some(2),
					height: Some(3),
					placeholder: Some("LKO2".to_string()),
					alt: Some("dot".to_string()),
				},
			},
			ChatMediaItem::File {
				resource: ChatResource::Preimage { key: vec![1; 32] },
				meta: ChatMediaMeta {
					mime_type: "application/pdf".to_string(),
					size_bytes: None,
					width: None,
					height: None,
					placeholder: None,
					alt: None,
				},
				file_name: "a.pdf".to_string(),
			},
		];
		let view = body_json(&ChatMessageBody::RichText { text: Some("**b**".to_string()), media });
		assert_eq!(view["type"], "RichText");
		assert_eq!(view["text"], "**b**");
		assert_eq!(
			view["media"][0],
			json!({
				"kind": "Image",
				"resource": { "type": "Inline", "mimeType": "image/png", "size": 1, "dataUrl": "data:image/png;base64,/w==" },
				"fileName": null,
				"meta": { "mimeType": "image/png", "sizeBytes": 1, "width": 2, "height": 3, "placeholder": "LKO2", "alt": "dot" },
			})
		);
		assert_eq!(view["media"][1]["kind"], "File");
		assert_eq!(view["media"][1]["fileName"], "a.pdf");
		assert_eq!(
			view["media"][1]["resource"],
			json!({ "type": "Preimage", "key": format!("0x{}", "01".repeat(32)) })
		);
		assert_eq!(
			body_json(&ChatMessageBody::Custom {
				message_type: "card".to_string(),
				payload: vec![0xde, 0xad],
				preview_text: None,
			}),
			json!({ "type": "Custom", "messageType": "card", "payloadHex": "dead", "previewText": null })
		);
	}

	#[test]
	fn first_version_content_is_shown_with_every_field_the_product_sent() {
		use truapi::latest::{
			ChatAction, ChatActions, ChatCustomMessage, ChatFile, ChatMedia, ChatReaction,
			ChatRichText,
		};
		let reaction = ChatReaction { message_id: "m1".to_string(), emoji: "+1".to_string() };
		let cases = [
			(text("a\nb"), json!({"type": "Text", "text": "a\nb"})),
			(
				ChatMessageContent::RichText(ChatRichText {
					text: None,
					media: vec![ChatMedia { url: "https://x/1.png".to_string() }],
				}),
				json!({"type": "RichText", "text": null, "media": [{"url": "https://x/1.png"}]}),
			),
			(
				ChatMessageContent::Actions(ChatActions {
					text: Some("pick".to_string()),
					actions: vec![ChatAction {
						action_id: "claim".to_string(),
						title: "Claim".to_string(),
					}],
					layout: ChatActionLayout::Grid,
				}),
				json!({
					"type": "Actions",
					"text": "pick",
					"actions": [{"actionId": "claim", "title": "Claim"}],
					"layout": "Grid",
				}),
			),
			(
				ChatMessageContent::File(ChatFile {
					url: "https://x/f".to_string(),
					file_name: "f.pdf".to_string(),
					mime_type: "application/pdf".to_string(),
					size_bytes: 42,
					text: None,
				}),
				json!({
					"type": "File",
					"url": "https://x/f",
					"fileName": "f.pdf",
					"mimeType": "application/pdf",
					"sizeBytes": 42,
					"text": null,
				}),
			),
			(
				ChatMessageContent::Reaction(reaction.clone()),
				json!({"type": "Reaction", "messageId": "m1", "emoji": "+1"}),
			),
			(
				ChatMessageContent::ReactionRemoved(reaction),
				json!({"type": "ReactionRemoved", "messageId": "m1", "emoji": "+1"}),
			),
			(
				ChatMessageContent::Custom(ChatCustomMessage {
					message_type: "card".to_string(),
					payload: vec![0xde, 0xad],
				}),
				json!({"type": "Custom", "messageType": "card", "payloadHex": "dead"}),
			),
		];
		for (content, expected) in cases {
			assert_eq!(content_json(&content), expected);
		}
	}

	// S49: the host lists what the page serves end to end, and nothing more.
	#[test]
	fn the_host_lists_the_features_the_page_serves() {
		let host = CliChatHost::new(None);

		let listed = futures::executor::block_on(host.chat_capabilities(&product()))
			.expect("the host lists its features");

		assert_eq!(listed.names, CLI_CHAT_FEATURES.map(str::to_string));
		// In the order of the RFC, as the core gives them to the product, so
		// the page's line and the product's read compare as they are.
		let order: Vec<_> = truapi::platform::CHAT_FEATURE_NAMES
			.iter()
			.filter(|name| CLI_CHAT_FEATURES.contains(name))
			.collect();
		assert_eq!(order, CLI_CHAT_FEATURES.iter().collect::<Vec<_>>());
		assert!(
			CLI_CHAT_FEATURES
				.iter()
				.all(|name| truapi::platform::CHAT_FEATURE_NAMES.contains(name)),
			"every name is a name of the RFC"
		);
	}

	// S51: the features of a room stay whole for the life of the host, a
	// feature the page does not serve included, so that a worker that starts
	// again reads them back.
	#[test]
	fn the_features_of_a_room_stay_whole() {
		let host = host_with_rooms(None);
		let features = ChatRoomFeatures {
			preimage_attachments: true,
			read_aloud: true,
			calls: Some(truapi::latest::ChatRoomCalls { audio: true, video: false }),
			..ChatRoomFeatures::default()
		};

		futures::executor::block_on(host.set_chat_room_features(
			&product(),
			HostChatSetRoomFeaturesRequest {
				room_id: "support".to_string(),
				features: features.clone(),
			},
		))
		.expect("the room exists");

		assert_eq!(
			futures::executor::block_on(
				host.read_chat_room_features(&product(), "support".to_string())
			),
			Ok(features)
		);
		assert_eq!(
			room_view(&host, "support")["features"],
			json!({
				"preimageAttachments": true,
				"search": false,
				"calls": { "audio": true, "video": false },
				"threadOf": null,
				"readAloud": true,
			})
		);
		assert_eq!(
			futures::executor::block_on(
				host.read_chat_room_features(&product(), "kept".to_string())
			),
			Ok(ChatRoomFeatures::default())
		);
	}

	fn set_meta(
		host: &CliChatHost,
		room_id: &str,
		message_id: &str,
		meta: ChatMessageMeta,
	) -> Result<(), HostChatUpdateFeedError> {
		futures::executor::block_on(host.set_chat_message_meta(
			&product(),
			HostChatSetMessageMetaRequest {
				room_id: room_id.to_string(),
				message_id: message_id.to_string(),
				meta,
			},
		))
	}

	fn branch(index: u16, count: u16) -> ChatMessageMeta {
		ChatMessageMeta {
			branch: Some(truapi::latest::ChatBranchPosition { index, count }),
			..ChatMessageMeta::default()
		}
	}

	// A room the host keeps holds meta for its own messages; a room the
	// product keeps holds meta before the page with the message arrives.
	#[test]
	fn the_meta_of_a_message_follows_the_room_mode() {
		let host = host_with_rooms(None);
		feed(&host, "support", ChatFeedUpdate::New(message("m1", PRODUCT, "hi")), true)
			.expect("stored");

		assert_eq!(
			set_meta(&host, "support", "m2", branch(1, 2)),
			Err(HostChatUpdateFeedError::UnknownMessage)
		);
		assert_eq!(set_meta(&host, "support", "m1", branch(1, 2)), Ok(()));
		assert_eq!(set_meta(&host, "kept", "unseen", branch(2, 2)), Ok(()));
		assert_eq!(
			room_view(&host, "support")["meta"]["m1"]["branch"],
			json!({ "index": 1, "count": 2 })
		);
		assert_eq!(
			room_view(&host, "kept")["meta"]["unseen"]["branch"],
			json!({ "index": 2, "count": 2 })
		);

		// The meta of a deleted message of the host's store goes with it.
		feed(&host, "support", ChatFeedUpdate::Deleted { message_id: "m1".to_string() }, true)
			.expect("deleted");
		assert_eq!(room_view(&host, "support")["meta"], json!({}));
	}

	#[test]
	fn a_room_the_product_keeps_holds_the_newest_meta_only() {
		let host = host_with_rooms(None);
		for index in 0..=CHAT_META_KEPT_MAX {
			set_meta(&host, "kept", &format!("m{index}"), branch(1, 1)).expect("kept");
		}

		let meta = room_view(&host, "kept")["meta"].as_object().expect("an object").clone();
		assert_eq!(meta.len(), CHAT_META_KEPT_MAX);
		assert!(!meta.contains_key("m0"));
		assert!(meta.contains_key(&format!("m{CHAT_META_KEPT_MAX}")));
	}

	#[test]
	fn the_call_of_a_room_shows_until_the_product_clears_it() {
		let host = host_with_rooms(None);
		let call = |call| {
			futures::executor::block_on(host.set_chat_room_call(
				&product(),
				HostChatSetRoomCallRequest { room_id: "support".to_string(), call },
			))
		};

		call(Some(ChatRoomCall::Ringing {
			call_id: "c1".to_string(),
			video: true,
			caller: Some("p1".to_string()),
		}))
		.expect("the room exists");
		assert_eq!(
			room_view(&host, "support")["call"],
			json!({ "type": "Ringing", "callId": "c1", "video": true, "caller": "p1" })
		);
		call(None).expect("the room exists");
		assert_eq!(room_view(&host, "support")["call"], Value::Null);
	}

	#[test]
	fn the_user_state_of_a_room_reads_back() {
		let host = host_with_rooms(None);

		host.set_user_state("support", Some(u64::MAX), false).expect("the room exists");
		host.set_user_state("kept", None, true).expect("the room exists");

		assert_eq!(
			futures::executor::block_on(host.read_chat_room_user_states(&product()))
				.expect("read")
				.rooms,
			[
				ChatRoomUserState {
					room_id: "kept".to_string(),
					muted_until: None,
					archived: true
				},
				ChatRoomUserState {
					room_id: "support".to_string(),
					muted_until: Some(u64::MAX),
					archived: false,
				},
			]
		);
		host.set_user_state("support", None, false).expect("the room exists");
		assert_eq!(host.user_states().len(), 1);
		assert!(host.set_user_state("gone", None, true).is_err());
	}

	// S54: the page lists what the host delivered, so it can show that a
	// notification into a muted room never came.
	#[test]
	fn a_delivered_notification_reaches_every_page() {
		let host = CliChatHost::new(None);
		let (_, mut page) = host.observe();

		host.notification_shown(3, "Ping", Some("polkadot://chat.dot/-/chat/main"));

		assert_eq!(
			page.try_recv().expect("an event"),
			json!({
				"kind": "notification",
				"id": 3,
				"text": "Ping",
				"deeplink": "polkadot://chat.dot/-/chat/main",
			})
		);
	}

	// S64: deleting a parent room deletes its thread rooms, which no list
	// shows, so none stays hidden for ever.
	#[test]
	fn deleting_a_room_deletes_its_threads() {
		let host = host_with_rooms(None);
		create(&host, room("support-thread"));
		futures::executor::block_on(host.set_chat_room_features(
			&product(),
			HostChatSetRoomFeaturesRequest {
				room_id: "support-thread".to_string(),
				features: ChatRoomFeatures {
					thread_of: Some(truapi::latest::ChatThreadParent {
						room_id: "support".to_string(),
						message_id: "m1".to_string(),
					}),
					..ChatRoomFeatures::default()
				},
			},
		))
		.expect("the thread room exists");

		futures::executor::block_on(host.delete_chat_room(
			&product(),
			HostChatDeleteRoomRequest { room_id: "support".to_string() },
		))
		.expect("deleted");

		let rooms: Vec<_> =
			host.observe().0.rooms.iter().map(|room| room["roomId"].clone()).collect();
		assert_eq!(rooms, [json!("kept")]);
	}

	// S66: a reasoning row is never the list preview.
	#[test]
	fn a_reasoning_row_is_not_the_list_preview() {
		let host = host_with_rooms(None);
		feed(&host, "support", ChatFeedUpdate::New(message("m1", PRODUCT, "the answer")), true)
			.expect("stored");
		feed(&host, "support", ChatFeedUpdate::New(message("m2", PRODUCT, "thinking")), true)
			.expect("stored");
		set_meta(
			&host,
			"support",
			"m2",
			ChatMessageMeta {
				reasoning: Some(truapi::latest::ChatReasoning { duration_ms: Some(4000) }),
				..ChatMessageMeta::default()
			},
		)
		.expect("kept");

		assert_eq!(room_view(&host, "support")["list"]["text"]["text"], "the answer");
	}

	// D30: a repeat registration keeps the persona and replaces its name.
	#[test]
	fn a_repeat_registration_replaces_the_persona() {
		let host = CliChatHost::new(None);
		let register = |name: &str| {
			futures::executor::block_on(host.register_chat_bot(
				&product(),
				HostChatRegisterBotRequest {
					bot_id: "helper".to_string(),
					name: name.to_string(),
					icon: String::new(),
				},
			))
			.expect("registered")
			.status
		};

		assert_eq!(register("Helper"), ChatBotRegistrationStatus::New);
		assert_eq!(register("Helper 2"), ChatBotRegistrationStatus::Exists);
		assert_eq!(
			futures::executor::block_on(host.read_chat_bots(&product()))
				.expect("read")
				.bot_ids,
			["helper"]
		);
		assert_eq!(
			host.observe().0.bots,
			[json!({ "botId": "helper", "name": "Helper 2", "icon": "" })]
		);
	}
}
