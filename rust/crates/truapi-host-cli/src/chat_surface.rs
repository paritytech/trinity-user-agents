//! Browser surface for the chat a development worker holds.
//!
//! `truapi-host dev` stands in for the phone a chat product runs beside. The
//! worker connects on [`WORKER_PATH`] and talks to the in-memory
//! [`CliChatHost`]; this module serves a page on [`PAGE_PATH`] that draws the
//! rooms the worker declared and lets a person use them as a phone user does.
//!
//! The page speaks a JSON protocol on [`SOCKET_PATH`]. The host sends a
//! snapshot, then every change to a room or a message. The page sends one
//! operation per thing the person does. An operation that the product only
//! hears about becomes a Chat event, which reaches every live worker through
//! `chat.actionSubscribe()`. An operation that needs an answer from the
//! product becomes one of the host-initiated queries, which the newest live
//! worker answers within the time-outs of the RFC; the answer goes back to
//! the page that asked, tagged with the page's request id.

use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
	sync::{Arc, Mutex, Weak},
	time::Duration,
};

use anyhow::Result;
use futures::future::BoxFuture;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
	io::{AsyncRead, AsyncWrite},
	sync::{mpsc, watch},
	time::Instant,
};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};
use truapi::{
	CallError, ProductRuntime, ProductRuntimeError, Subscription,
	latest::{
		ActionTrigger, ChatBuiltInMessageAction, ChatCommand, ChatEvent, ChatMessageBody,
		ChatMessageMenu, ChatPageAnchor, ChatParticipant, ChatParticipantEntry,
		ChatParticipantMenu, ChatParticipantsQuery, ChatProductAction, ChatRoomEvent,
		ChatUserAction, HostChatActionSubscribeItem, HostChatRoomEventsSubscribeItem,
		ProductChatMessageActionsQueryItem, ProductChatMessageActionsQueryRequest,
		ProductChatMessagesQueryItem, ProductChatMessagesQueryRequest,
		ProductChatParticipantActionsQueryItem, ProductChatParticipantActionsQueryRequest,
		ProductChatParticipantsQueryItem, ProductChatParticipantsQueryRequest,
		ProductChatQueryError, ProductChatReactionsQueryItem, ProductChatReactionsQueryRequest,
		ProductChatUserActionError, ProductChatUserActionItem, ProductChatUserActionRequest,
	},
};

use crate::chat::{
	ChatSnapshot, CliChatHost, PRODUCT, USER, UserChange, handler_json, message_json, resource_json,
};

/// Path of the chat surface page.
pub const PAGE_PATH: &str = "/chat";

/// Path of the WebSocket the page speaks the surface protocol on.
pub const SOCKET_PATH: &str = "/chat/ws";

/// Path of the product-frame WebSocket a worker opens.
pub const WORKER_PATH: &str = "/worker";

/// Path of the bridge script that points a page at [`WORKER_PATH`].
pub const WORKER_BOOTSTRAP_PATH: &str = "/worker/bootstrap.js";

/// Path the configured worker bundle is served from.
pub const WORKER_BUNDLE_PATH: &str = "/worker/index.js";

/// Browser URL of the chat page for a frame endpoint that has one. A private
/// Unix socket has no browser-facing address.
pub fn page_url(frame_url: &str) -> Option<String> {
	let authority = frame_url.strip_prefix("ws://")?;
	Some(format!("http://{authority}{PAGE_PATH}"))
}

/// Peer every surface action is attributed to, matching the iOS host.
const PEER: &str = "native";

const PAGE: &str = include_str!("chat_surface.html");

/// Placeholder in [`PAGE`] where the worker's scripts go. It sits after the
/// page's own script, which has to capture `WebSocket` before the bridge
/// script installs the product sandbox over it.
const WORKER_BOOT_MARKER: &str = "<!--WORKER_BOOT-->";

const WORKER_BOOT: &str = "<script src=\"/worker/bootstrap.js\"></script>\n\
     <script type=\"module\" src=\"/worker/index.js\"></script>";

/// One host-initiated query, as the core opens it: at most one item, then the
/// end.
pub type Query<Item, Error = ProductChatQueryError> = Subscription<Item, CallError<Error>>;

/// A worker connection the surface delivers events to and asks queries of.
pub trait ChatWorker: Send + Sync {
	/// Queue `item` for the product's `chat.actionSubscribe()`.
	fn publish_chat_event(
		&self,
		item: HostChatActionSubscribeItem,
	) -> Result<(), ProductRuntimeError>;

	/// Ask one page of messages of a room the product keeps.
	fn messages_query(
		&self,
		request: ProductChatMessagesQueryRequest,
	) -> Result<Query<ProductChatMessagesQueryItem>, ProductRuntimeError>;

	/// Tell the product what the user did in a room it keeps.
	fn user_action(
		&self,
		request: ProductChatUserActionRequest,
	) -> Result<Query<ProductChatUserActionItem, ProductChatUserActionError>, ProductRuntimeError>;

	/// Ask who reacted to one message.
	fn reactions_query(
		&self,
		request: ProductChatReactionsQueryRequest,
	) -> Result<Query<ProductChatReactionsQueryItem>, ProductRuntimeError>;

	/// Ask about the participants of a room.
	fn participants_query(
		&self,
		request: ProductChatParticipantsQueryRequest,
	) -> Result<Query<ProductChatParticipantsQueryItem>, ProductRuntimeError>;

	/// Ask the menu of one message.
	fn message_actions_query(
		&self,
		request: ProductChatMessageActionsQueryRequest,
	) -> Result<Query<ProductChatMessageActionsQueryItem>, ProductRuntimeError>;

	/// Ask the menu of one participant.
	fn participant_actions_query(
		&self,
		request: ProductChatParticipantActionsQueryRequest,
	) -> Result<Query<ProductChatParticipantActionsQueryItem>, ProductRuntimeError>;

	/// Send one room event to the product's `chat.roomEventsSubscribe()`. The
	/// core keeps it for the product until a connection subscribes.
	fn publish_chat_room_event(
		&self,
		item: HostChatRoomEventsSubscribeItem,
	) -> Result<(), ProductRuntimeError>;
}

impl ChatWorker for ProductRuntime {
	fn publish_chat_event(
		&self,
		item: HostChatActionSubscribeItem,
	) -> Result<(), ProductRuntimeError> {
		self.control().publish_chat_event(item)
	}

	fn messages_query(
		&self,
		request: ProductChatMessagesQueryRequest,
	) -> Result<Query<ProductChatMessagesQueryItem>, ProductRuntimeError> {
		self.control().chat_messages_query(request)
	}

	fn user_action(
		&self,
		request: ProductChatUserActionRequest,
	) -> Result<Query<ProductChatUserActionItem, ProductChatUserActionError>, ProductRuntimeError>
	{
		self.control().chat_user_action(request)
	}

	fn reactions_query(
		&self,
		request: ProductChatReactionsQueryRequest,
	) -> Result<Query<ProductChatReactionsQueryItem>, ProductRuntimeError> {
		self.control().chat_reactions_query(request)
	}

	fn participants_query(
		&self,
		request: ProductChatParticipantsQueryRequest,
	) -> Result<Query<ProductChatParticipantsQueryItem>, ProductRuntimeError> {
		self.control().chat_participants_query(request)
	}

	fn message_actions_query(
		&self,
		request: ProductChatMessageActionsQueryRequest,
	) -> Result<Query<ProductChatMessageActionsQueryItem>, ProductRuntimeError> {
		self.control().chat_message_actions_query(request)
	}

	fn participant_actions_query(
		&self,
		request: ProductChatParticipantActionsQueryRequest,
	) -> Result<Query<ProductChatParticipantActionsQueryItem>, ProductRuntimeError> {
		self.control().chat_participant_actions_query(request)
	}

	fn publish_chat_room_event(
		&self,
		item: HostChatRoomEventsSubscribeItem,
	) -> Result<(), ProductRuntimeError> {
		self.control().publish_chat_room_event(item)
	}
}

/// How long the host waits for each answer (RFC "Timeouts and failure
/// states").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
	/// `messages_query`.
	pub page: Duration,
	/// `user_action` with `NewMessage`.
	pub send: Duration,
	/// Every other `user_action`.
	pub action: Duration,
	/// `reactions_query`, `participants_query` and the two menu queries.
	pub query: Duration,
	/// The wait before the one retry of an `unavailable` interrupt.
	pub retry: Duration,
}

impl Default for Timeouts {
	fn default() -> Self {
		Self {
			page: Duration::from_secs(10),
			send: Duration::from_secs(30),
			action: Duration::from_secs(10),
			query: Duration::from_secs(5),
			retry: Duration::from_secs(1),
		}
	}
}

/// Live worker connections, keyed by attachment order.
#[derive(Default)]
struct Workers {
	next: u64,
	live: BTreeMap<u64, Weak<dyn ChatWorker>>,
}

/// The development chat surface for one host.
pub struct ChatSurface {
	chat: Arc<CliChatHost>,
	worker_bundle: Option<PathBuf>,
	product_id: String,
	workers: Mutex<Workers>,
	/// Number of live workers, for every open page.
	connected: watch::Sender<usize>,
	timeouts: Timeouts,
}

/// Keeps a worker attached to the surface until dropped.
pub struct WorkerAttachment {
	surface: Arc<ChatSurface>,
	id: u64,
}

impl Drop for WorkerAttachment {
	fn drop(&mut self) {
		let mut workers = self.surface.lock_workers();
		workers.live.remove(&self.id);
		self.surface.connected.send_replace(workers.live.len());
	}
}

/// What the page asks about the participants of a room.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
enum ParticipantsOp {
	Count,
	Page { cursor: Option<String>, page_size: u32 },
	ByIds { participant_ids: Vec<String> },
}

/// An operation a page sends, tagged on `op`. `reqId` is the page's own
/// number for an operation that gets an `answer`.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum PageOp {
	/// The user sends a text, as a message of the room.
	Post {
		#[serde(default)]
		req_id: u64,
		room_id: String,
		text: String,
		#[serde(default)]
		reply_to: Option<String>,
	},
	/// A press on a button of an `Actions` message.
	Trigger {
		room_id: String,
		message_id: String,
		action_id: String,
	},
	/// A slash command the room declared.
	Command {
		room_id: String,
		command: String,
		payload: String,
	},
	Edit {
		#[serde(default)]
		req_id: u64,
		room_id: String,
		message_id: String,
		text: String,
	},
	Delete {
		#[serde(default)]
		req_id: u64,
		room_id: String,
		message_id: String,
	},
	React {
		#[serde(default)]
		req_id: u64,
		room_id: String,
		message_id: String,
		emoji: String,
		placed: bool,
	},
	/// The newest message on screen.
	Seen {
		#[serde(default)]
		req_id: u64,
		room_id: String,
		message_id: String,
	},
	/// Send a `Failed` message again, in a room the product keeps.
	Retry {
		#[serde(default)]
		req_id: u64,
		room_id: String,
		message_id: String,
	},
	MessageAction {
		room_id: String,
		message_id: String,
		action_id: String,
	},
	ParticipantAction {
		room_id: String,
		participant_id: String,
		action_id: String,
	},
	Toolbar {
		room_id: String,
		action_id: String,
	},
	Suggestion {
		room_id: String,
		suggestion_id: String,
	},
	/// The room screen opened.
	Open {
		room_id: String,
	},
	/// The room screen closed.
	Close {
		room_id: String,
	},
	/// One page of a room the product keeps.
	Messages {
		req_id: u64,
		room_id: String,
		before: bool,
		#[serde(default)]
		cursor: Option<String>,
		page_size: u32,
	},
	Reactions {
		req_id: u64,
		room_id: String,
		message_id: String,
	},
	Participants {
		req_id: u64,
		room_id: String,
		query: ParticipantsOp,
	},
	MessageActions {
		req_id: u64,
		room_id: String,
		message_id: String,
	},
	ParticipantActions {
		req_id: u64,
		room_id: String,
		participant_id: String,
	},
	/// The user muted, unmuted, archived or unarchived the room.
	/// `muted_forever` mutes the room with no end, a time a page cannot write
	/// as a JSON number.
	UserState {
		room_id: String,
		#[serde(default)]
		muted_until: Option<u64>,
		#[serde(default)]
		muted_forever: bool,
		#[serde(default)]
		archived: bool,
	},
}

/// The reply to one page operation: at once, or once the product answered.
enum Applied {
	Now(Value),
	Later(BoxFuture<'static, Value>),
}

/// Why a query got no item.
enum Unanswered<E> {
	/// The product or the core interrupted it.
	Refused(CallError<E>),
	/// The product sent nothing within the time-out.
	Timeout,
	/// No worker was connected within the time-out.
	NoWorker,
	/// The core refused to open the query.
	Closed(String),
}

impl ChatSurface {
	/// Build a surface over `chat` for `product_id`, serving the worker bundle
	/// at `worker_bundle` when one is configured.
	pub fn new(
		chat: Arc<CliChatHost>,
		worker_bundle: Option<PathBuf>,
		product_id: String,
	) -> Arc<Self> {
		Self::with_timeouts(chat, worker_bundle, product_id, Timeouts::default())
	}

	/// [`ChatSurface::new`] with the time-outs `timeouts`.
	pub fn with_timeouts(
		chat: Arc<CliChatHost>,
		worker_bundle: Option<PathBuf>,
		product_id: String,
		timeouts: Timeouts,
	) -> Arc<Self> {
		Arc::new(Self {
			chat,
			worker_bundle,
			product_id,
			workers: Mutex::new(Workers::default()),
			connected: watch::channel(0).0,
			timeouts,
		})
	}

	/// The chat host the worker's product runtimes are built with.
	pub fn chat(&self) -> &Arc<CliChatHost> {
		&self.chat
	}

	/// The worker bundle the page boots, if one was configured.
	pub fn worker_bundle_path(&self) -> Option<&Path> {
		self.worker_bundle.as_deref()
	}

	fn worker_configured(&self) -> bool {
		self.worker_bundle.is_some()
	}

	/// How many worker connections are attached now.
	#[cfg(test)]
	pub fn connected_workers(&self) -> usize {
		*self.connected.borrow()
	}

	fn lock_workers(&self) -> std::sync::MutexGuard<'_, Workers> {
		self.workers.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// Deliver the surface's events and queries to `worker` until the
	/// returned attachment is dropped.
	pub fn attach_worker(self: &Arc<Self>, worker: &Arc<dyn ChatWorker>) -> WorkerAttachment {
		let mut workers = self.lock_workers();
		let id = workers.next;
		workers.next += 1;
		workers.live.insert(id, Arc::downgrade(worker));
		self.connected.send_replace(workers.live.len());
		WorkerAttachment { surface: Arc::clone(self), id }
	}

	/// The newest live worker, which answers the queries.
	fn newest_worker(&self) -> Option<Arc<dyn ChatWorker>> {
		self.lock_workers().live.values().rev().find_map(Weak::upgrade)
	}

	/// The newest live worker. When no worker is live, wait until `deadline` for
	/// one to attach.
	/// A worker that boots with the page attaches a moment after the page
	/// asks its first question.
	async fn worker_by(&self, deadline: Instant) -> Option<Arc<dyn ChatWorker>> {
		let mut connected = self.connected.subscribe();
		loop {
			if let Some(worker) = self.newest_worker() {
				return Some(worker);
			}
			match tokio::time::timeout_at(deadline, connected.changed()).await {
				Ok(Ok(())) => continue,
				_ => return None,
			}
		}
	}

	/// The page, booting the worker from it only when a bundle is configured.
	pub fn page(&self) -> String {
		let boot = if self.worker_configured() { WORKER_BOOT } else { "" };
		PAGE.replacen(WORKER_BOOT_MARKER, boot, 1)
	}

	/// The worker bundle as it is on disk now, or the reason there is none.
	/// Read per request because a watch build rewrites it underneath.
	pub fn worker_bundle(&self) -> Result<String, String> {
		let Some(path) = self.worker_bundle.as_ref() else {
			return Err(format!(
				"no worker bundle is configured; pass `truapi-host dev --worker-bundle <PATH>` to serve one at {WORKER_BUNDLE_PATH}\n"
			));
		};
		std::fs::read_to_string(path).map_err(|error| {
			format!(
				"worker bundle {} cannot be read ({error}); is its build still running?\n",
				path.display()
			)
		})
	}

	/// Serve one page connection: send the snapshot, then every chat change,
	/// worker change and answer, and apply each operation the page sends.
	pub async fn serve_ws<S>(self: &Arc<Self>, ws: WebSocketStream<S>) -> Result<()>
	where
		S: AsyncRead + AsyncWrite + Unpin,
	{
		let (snapshot, mut events) = self.chat.observe();
		let mut connected = self.connected.subscribe();
		let (answers, mut answered) = mpsc::unbounded_channel::<Value>();
		let (mut write, mut read) = ws.split();
		let workers = *connected.borrow_and_update();
		write.send(frame(&self.snapshot(snapshot, workers))).await?;
		loop {
			let reply = tokio::select! {
				event = events.next() => match event {
					Some(event) => event,
					None => break,
				},
				changed = connected.changed() => {
					if changed.is_err() {
						break;
					}
					json!({ "kind": "workers", "connected": *connected.borrow_and_update() })
				}
				answer = answered.recv() => match answer {
					Some(answer) => answer,
					None => break,
				},
				message = read.next() => match message {
					Some(Ok(Message::Text(text))) => match self.apply(&text) {
						Applied::Now(reply) => reply,
						Applied::Later(answer) => {
							let answers = answers.clone();
							// The page goes on while the product thinks; the
							// answer comes back tagged with its request id.
							tokio::spawn(async move {
								let _ = answers.send(answer.await);
							});
							continue;
						},
					},
					Some(Ok(Message::Close(_)) | Err(_)) | None => break,
					Some(Ok(_)) => continue,
				},
			};
			write.send(frame(&reply)).await?;
		}
		Ok(())
	}

	fn snapshot(&self, snapshot: ChatSnapshot, workers: usize) -> Value {
		json!({
			"kind": "snapshot",
			"productId": self.product_id,
			"peer": PEER,
			"worker": { "configured": self.worker_configured(), "connected": workers },
			"timeouts": {
				"page": self.timeouts.page.as_millis() as u64,
				"send": self.timeouts.send.as_millis() as u64,
				"action": self.timeouts.action.as_millis() as u64,
				"query": self.timeouts.query.as_millis() as u64,
			},
			"capabilities": {
				"features": crate::chat::CLI_CHAT_FEATURES,
				"inlineAttachmentMaxBytes": truapi::platform::CHAT_INLINE_MEDIA_MAX_BYTES,
				"preimageAttachmentMaxBytes": 0,
			},
			"rooms": snapshot.rooms,
			"bots": snapshot.bots,
			"messages": snapshot.messages,
		})
	}

	/// Apply one page operation. A reply at once carries the request id of
	/// an operation that has one, so the page can match a refusal to it.
	fn apply(self: &Arc<Self>, text: &str) -> Applied {
		let op = match serde_json::from_str::<PageOp>(text) {
			Ok(op) => op,
			Err(error) => return Applied::Now(error_reply(format!("malformed op: {error}"))),
		};
		match (self.apply_op(&op), op_req_id(&op)) {
			(Applied::Now(mut reply), Some(req_id)) if reply.get("reqId").is_none() => {
				reply["reqId"] = Value::from(req_id);
				Applied::Now(reply)
			},
			(applied, _) => applied,
		}
	}

	fn apply_op(self: &Arc<Self>, op: &PageOp) -> Applied {
		let room_id = op_room(op).to_string();
		let Some(config) = self.chat.room_config(&room_id) else {
			return Applied::Now(unknown_room(&room_id));
		};
		let source = config.product_source;
		let event = |event: ChatEvent| Applied::Now(self.deliver(op_name(op), &room_id, event));
		match op {
			PageOp::Post { req_id, text, reply_to, .. } => {
				let body = ChatMessageBody::Text { text: text.clone() };
				if source {
					let action = ChatUserAction::NewMessage { body, reply_to: reply_to.clone() };
					return self.later_action("post", *req_id, room_id, action);
				}
				// Stored first, so the user's message precedes any reply the
				// worker posts to it.
				match self.chat.post_user_message(&room_id, body.clone(), reply_to.clone()) {
					Ok(message) => {
						let posted = ChatEvent::MessagePosted {
							message_id: message.message_id.clone(),
							body,
							reply_to: reply_to.clone(),
						};
						let delivered = self.deliver("post", &room_id, posted);
						Applied::Now(answer_ok(
							"post",
							*req_id,
							json!({
								"type": "Posted",
								"message": message_json(&message),
								"workers": delivered["workers"],
							}),
						))
					},
					Err(reason) => Applied::Now(error_reply(reason)),
				}
			},
			PageOp::Trigger { message_id, action_id, .. } => {
				event(ChatEvent::ActionTriggered(ActionTrigger {
					message_id: message_id.clone(),
					action_id: action_id.clone(),
					payload: None,
				}))
			},
			PageOp::Command { command, payload, .. } => {
				// Only a declared name after `/` is a command.
				if !self.chat.declares_command(&room_id, command) {
					return Applied::Now(error_reply(format!(
						"the room declares no command {command:?}; send it as a message"
					)));
				}
				event(ChatEvent::Command(ChatCommand {
					command: command.clone(),
					payload: payload.clone(),
				}))
			},
			PageOp::Edit { req_id, message_id, text, .. } => {
				let body = ChatMessageBody::Text { text: text.clone() };
				if source {
					let action =
						ChatUserAction::EditMessage { message_id: message_id.clone(), body };
					return self.later_action("edit", *req_id, room_id, action);
				}
				let change =
					UserChange::Edit { message_id: message_id.clone(), body: body.clone() };
				self.host_change(
					*req_id,
					&room_id,
					change,
					ChatEvent::UserEdited { message_id: message_id.clone(), body },
				)
			},
			PageOp::Delete { req_id, message_id, .. } => {
				if source {
					let action = ChatUserAction::DeleteMessage { message_id: message_id.clone() };
					return self.later_action("delete", *req_id, room_id, action);
				}
				let change = UserChange::Delete { message_id: message_id.clone() };
				self.host_change(
					*req_id,
					&room_id,
					change,
					ChatEvent::UserDeleted { message_id: message_id.clone() },
				)
			},
			PageOp::React { req_id, message_id, emoji, placed, .. } => {
				if let Some(allowed) = &config.allowed_reactions &&
					*placed && !allowed.contains(emoji)
				{
					return Applied::Now(error_reply(format!(
						"the room does not offer the reaction {emoji}"
					)));
				}
				if source {
					let (message_id, emoji) = (message_id.clone(), emoji.clone());
					let action = if *placed {
						ChatUserAction::ReactionPlaced { message_id, emoji }
					} else {
						ChatUserAction::ReactionRemoved { message_id, emoji }
					};
					return self.later_action("react", *req_id, room_id, action);
				}
				let change = UserChange::React {
					message_id: message_id.clone(),
					emoji: emoji.clone(),
					placed: *placed,
				};
				self.host_change(
					*req_id,
					&room_id,
					change,
					ChatEvent::UserReacted {
						message_id: message_id.clone(),
						emoji: emoji.clone(),
						placed: *placed,
					},
				)
			},
			PageOp::Seen { req_id, message_id, .. } => {
				if source {
					if !config.report_seen {
						return Applied::Now(answer_ok("seen", *req_id, json!({ "type": "Kept" })));
					}
					let action = ChatUserAction::Seen { message_id: message_id.clone() };
					return self.later_action("seen", *req_id, room_id, action);
				}
				match self.chat.mark_seen(&room_id, message_id) {
					Ok(true) => event(ChatEvent::Seen { message_id: message_id.clone() }),
					Ok(false) => {
						Applied::Now(answer_ok("seen", *req_id, json!({ "type": "Kept" })))
					},
					Err(reason) => Applied::Now(error_reply(reason)),
				}
			},
			PageOp::Retry { req_id, message_id, .. } => {
				if !source {
					return Applied::Now(error_reply(
						"retry exists only in a room the product keeps; delete the message instead"
							.to_string(),
					));
				}
				let action = ChatUserAction::Retry { message_id: message_id.clone() };
				self.later_action("retry", *req_id, room_id, action)
			},
			PageOp::MessageAction { message_id, action_id, .. } => {
				event(ChatEvent::MessageActionTriggered {
					message_id: message_id.clone(),
					action_id: action_id.clone(),
				})
			},
			PageOp::ParticipantAction { participant_id, action_id, .. } => {
				event(ChatEvent::ParticipantActionTriggered {
					participant_id: participant_id.clone(),
					action_id: action_id.clone(),
				})
			},
			PageOp::Toolbar { action_id, .. } => {
				event(ChatEvent::ToolbarActionTriggered { action_id: action_id.clone() })
			},
			PageOp::Suggestion { suggestion_id, .. } => {
				event(ChatEvent::SuggestionSelected { suggestion_id: suggestion_id.clone() })
			},
			PageOp::Open { .. } => event(ChatEvent::RoomOpened),
			PageOp::Close { .. } => {
				self.chat.close_room(&room_id);
				event(ChatEvent::RoomClosed)
			},
			PageOp::Messages { req_id, before, cursor, page_size, .. } => {
				let cursor = cursor.clone();
				let anchor = if *before {
					ChatPageAnchor::Before { cursor }
				} else {
					ChatPageAnchor::After { cursor }
				};
				let request = ProductChatMessagesQueryRequest {
					room_id: room_id.clone(),
					anchor,
					page_size: *page_size,
				};
				let surface = Arc::clone(self);
				let req_id = *req_id;
				Applied::Later(Box::pin(async move {
					let answer = surface
						.ask(surface.timeouts.page, move |worker| {
							worker.messages_query(request.clone())
						})
						.await;
					if let Ok(page) = &answer {
						surface.chat.cache_page(&room_id, &page.messages);
					}
					answer_json("messages", req_id, answer, page_json, query_error_json)
				}))
			},
			PageOp::Reactions { req_id, message_id, .. } => {
				let request = ProductChatReactionsQueryRequest {
					room_id: room_id.clone(),
					message_id: message_id.clone(),
				};
				let surface = Arc::clone(self);
				let req_id = *req_id;
				Applied::Later(Box::pin(async move {
					let answer = surface
						.ask(surface.timeouts.query, move |worker| {
							worker.reactions_query(request.clone())
						})
						.await;
					answer_json("reactions", req_id, answer, reactions_json, query_error_json)
				}))
			},
			PageOp::Participants { req_id, query, .. } => {
				let query = match query {
					ParticipantsOp::Count => ChatParticipantsQuery::Count,
					ParticipantsOp::Page { cursor, page_size } => ChatParticipantsQuery::Page {
						cursor: cursor.clone(),
						page_size: *page_size,
					},
					// The reserved ids resolve without a question.
					ParticipantsOp::ByIds { participant_ids } => ChatParticipantsQuery::ByIds {
						participant_ids: participant_ids
							.iter()
							.filter(|id| *id != USER && *id != PRODUCT)
							.cloned()
							.collect(),
					},
				};
				let request =
					ProductChatParticipantsQueryRequest { room_id: room_id.clone(), query };
				let surface = Arc::clone(self);
				let req_id = *req_id;
				Applied::Later(Box::pin(async move {
					let answer = surface
						.ask(surface.timeouts.query, move |worker| {
							worker.participants_query(request.clone())
						})
						.await;
					if let Ok(
						ProductChatParticipantsQueryItem::Page { participants, .. } |
						ProductChatParticipantsQueryItem::Resolved { participants },
					) = &answer
					{
						surface.chat.cache_participants(&room_id, participants);
					}
					answer_json("participants", req_id, answer, participants_json, query_error_json)
				}))
			},
			PageOp::MessageActions { req_id, message_id, .. } => {
				let Some(message) = self.chat.message(&room_id, message_id) else {
					return Applied::Now(error_reply(format!("unknown message {message_id:?}")));
				};
				let request =
					ProductChatMessageActionsQueryRequest { room_id: room_id.clone(), message };
				let surface = Arc::clone(self);
				let req_id = *req_id;
				Applied::Later(Box::pin(async move {
					let answer = surface
						.ask(surface.timeouts.query, move |worker| {
							worker.message_actions_query(request.clone())
						})
						.await;
					answer_json(
						"messageActions",
						req_id,
						answer,
						|item| message_menu_json(&item.menu),
						query_error_json,
					)
				}))
			},
			PageOp::ParticipantActions { req_id, participant_id, .. } => {
				let Some(entry) = self.chat.participant(&room_id, participant_id) else {
					return Applied::Now(error_reply(format!(
						"unknown participant {participant_id:?}"
					)));
				};
				let request =
					ProductChatParticipantActionsQueryRequest { room_id: room_id.clone(), entry };
				let surface = Arc::clone(self);
				let req_id = *req_id;
				Applied::Later(Box::pin(async move {
					let answer = surface
						.ask(surface.timeouts.query, move |worker| {
							worker.participant_actions_query(request.clone())
						})
						.await;
					answer_json(
						"participantActions",
						req_id,
						answer,
						|item| participant_menu_json(&item.menu),
						query_error_json,
					)
				}))
			},
			PageOp::UserState { muted_until, muted_forever, archived, .. } => {
				let muted_until = if *muted_forever { Some(u64::MAX) } else { *muted_until };
				if let Err(reason) = self.chat.set_user_state(&room_id, muted_until, *archived) {
					return Applied::Now(error_reply(reason));
				}
				let event = ChatRoomEvent::UserStateChanged { muted_until, archived: *archived };
				Applied::Now(self.deliver_room_event("userState", &room_id, event))
			},
		}
	}

	/// Send one room event to the product through a live worker. Every
	/// worker connection of this host shares one queue for the product, so
	/// one connection is enough.
	fn deliver_room_event(&self, name: &str, room_id: &str, event: ChatRoomEvent) -> Value {
		let item = HostChatRoomEventsSubscribeItem { room_id: room_id.to_string(), event };
		let worker = self.lock_workers().live.values().rev().filter_map(Weak::upgrade).next();
		let Some(worker) = worker else {
			return error_reply("no worker is connected to receive the room event".to_string());
		};
		match worker.publish_chat_room_event(item) {
			Ok(()) => json!({ "kind": "delivered", "op": name, "workers": 1 }),
			Err(error) => error_reply(error.to_string()),
		}
	}

	/// Apply a change the user made in a room the host keeps, then tell the
	/// product.
	fn host_change(
		&self,
		req_id: u64,
		room_id: &str,
		change: UserChange,
		event: ChatEvent,
	) -> Applied {
		let name = match change {
			UserChange::Edit { .. } => "edit",
			UserChange::Delete { .. } => "delete",
			UserChange::React { .. } => "react",
		};
		match self.chat.change_as_user(room_id, change) {
			Ok(()) => {
				let mut delivered = self.deliver(name, room_id, event);
				delivered["reqId"] = Value::from(req_id);
				Applied::Now(delivered)
			},
			Err(reason) => Applied::Now(error_reply(reason)),
		}
	}

	/// Send one user action of a room the product keeps, and apply the answer
	/// to the screen.
	fn later_action(
		self: &Arc<Self>,
		name: &'static str,
		req_id: u64,
		room_id: String,
		action: ChatUserAction,
	) -> Applied {
		let surface = Arc::clone(self);
		let timeout = match action {
			ChatUserAction::NewMessage { .. } => self.timeouts.send,
			_ => self.timeouts.action,
		};
		Applied::Later(Box::pin(async move {
			let request = ProductChatUserActionRequest { room_id: room_id.clone(), action };
			let asked = request.clone();
			let answer =
				surface.ask(timeout, move |worker| worker.user_action(asked.clone())).await;
			if let Ok(item) = &answer {
				surface.chat.apply_source_action(&room_id, &request.action, item);
			}
			answer_json(name, req_id, answer, user_action_json, user_action_error_json)
		}))
	}

	/// Ask the newest worker one query and take its first item within
	/// `timeout`. An `unavailable` interrupt is asked again once, after the
	/// retry wait.
	async fn ask<Item, Error>(
		&self,
		timeout: Duration,
		open: impl Fn(&dyn ChatWorker) -> Result<Query<Item, Error>, ProductRuntimeError>,
	) -> Result<Item, Unanswered<Error>> {
		let deadline = Instant::now() + timeout;
		let mut retried = false;
		loop {
			let Some(worker) = self.worker_by(deadline).await else {
				return Err(Unanswered::NoWorker);
			};
			let mut query =
				open(worker.as_ref()).map_err(|error| Unanswered::Closed(error.to_string()))?;
			let answer = match tokio::time::timeout_at(deadline, query.next()).await {
				Err(_) => return Err(Unanswered::Timeout),
				Ok(answer) => answer,
			};
			match answer {
				Some(Ok(item)) => return Ok(item),
				Some(Err(CallError::HostFailure { reason }))
					if reason == "unavailable" && !retried =>
				{
					retried = true;
					let wait = Instant::now() + self.timeouts.retry;
					if wait >= deadline {
						return Err(Unanswered::Timeout);
					}
					tokio::time::sleep_until(wait).await;
				},
				Some(Err(error)) => return Err(Unanswered::Refused(error)),
				None => {
					return Err(Unanswered::Closed(
						"the product ended the query without an answer".to_string(),
					));
				},
			}
		}
	}

	/// Deliver one event to every live worker. The answer is the `delivered`
	/// reply or the `error` reply.
	fn deliver(&self, name: &str, room_id: &str, event: ChatEvent) -> Value {
		let item = HostChatActionSubscribeItem {
			room_id: room_id.to_string(),
			peer: PEER.to_string(),
			event,
		};
		match self.publish(item) {
			Ok(workers) => json!({ "kind": "delivered", "op": name, "workers": workers }),
			Err(error) => error_reply(error.to_string()),
		}
	}

	/// Publish `item` to every live worker, returning how many accepted it,
	/// or the first refusal once every worker has been offered it.
	fn publish(&self, item: HostChatActionSubscribeItem) -> Result<usize, ProductRuntimeError> {
		let workers: Vec<_> = self.lock_workers().live.values().filter_map(Weak::upgrade).collect();
		let mut accepted = 0;
		let mut refusal = None;
		for worker in workers {
			match worker.publish_chat_event(item.clone()) {
				Ok(()) => accepted += 1,
				Err(error) => {
					refusal.get_or_insert(error);
				},
			}
		}
		refusal.map_or(Ok(accepted), Err)
	}
}

fn op_room(op: &PageOp) -> &str {
	match op {
		PageOp::Post { room_id, .. } |
		PageOp::Trigger { room_id, .. } |
		PageOp::Command { room_id, .. } |
		PageOp::Edit { room_id, .. } |
		PageOp::Delete { room_id, .. } |
		PageOp::React { room_id, .. } |
		PageOp::Seen { room_id, .. } |
		PageOp::Retry { room_id, .. } |
		PageOp::MessageAction { room_id, .. } |
		PageOp::ParticipantAction { room_id, .. } |
		PageOp::Toolbar { room_id, .. } |
		PageOp::Suggestion { room_id, .. } |
		PageOp::Open { room_id } |
		PageOp::Close { room_id } |
		PageOp::Messages { room_id, .. } |
		PageOp::Reactions { room_id, .. } |
		PageOp::Participants { room_id, .. } |
		PageOp::MessageActions { room_id, .. } |
		PageOp::ParticipantActions { room_id, .. } |
		PageOp::UserState { room_id, .. } => room_id,
	}
}

fn op_req_id(op: &PageOp) -> Option<u64> {
	match op {
		PageOp::Post { req_id, .. } |
		PageOp::Edit { req_id, .. } |
		PageOp::Delete { req_id, .. } |
		PageOp::React { req_id, .. } |
		PageOp::Seen { req_id, .. } |
		PageOp::Retry { req_id, .. } |
		PageOp::Messages { req_id, .. } |
		PageOp::Reactions { req_id, .. } |
		PageOp::Participants { req_id, .. } |
		PageOp::MessageActions { req_id, .. } |
		PageOp::ParticipantActions { req_id, .. } => Some(*req_id),
		_ => None,
	}
}

fn op_name(op: &PageOp) -> &'static str {
	match op {
		PageOp::Post { .. } => "post",
		PageOp::Trigger { .. } => "trigger",
		PageOp::Command { .. } => "command",
		PageOp::Edit { .. } => "edit",
		PageOp::Delete { .. } => "delete",
		PageOp::React { .. } => "react",
		PageOp::Seen { .. } => "seen",
		PageOp::Retry { .. } => "retry",
		PageOp::MessageAction { .. } => "messageAction",
		PageOp::ParticipantAction { .. } => "participantAction",
		PageOp::Toolbar { .. } => "toolbar",
		PageOp::Suggestion { .. } => "suggestion",
		PageOp::Open { .. } => "open",
		PageOp::Close { .. } => "close",
		PageOp::Messages { .. } => "messages",
		PageOp::Reactions { .. } => "reactions",
		PageOp::Participants { .. } => "participants",
		PageOp::MessageActions { .. } => "messageActions",
		PageOp::ParticipantActions { .. } => "participantActions",
		PageOp::UserState { .. } => "userState",
	}
}

fn frame(value: &Value) -> Message {
	Message::Text(value.to_string())
}

fn error_reply(message: String) -> Value {
	json!({ "kind": "error", "message": message })
}

fn unknown_room(room_id: &str) -> Value {
	error_reply(format!("unknown room {room_id:?}"))
}

fn answer_ok(op: &str, req_id: u64, ok: Value) -> Value {
	json!({ "kind": "answer", "op": op, "reqId": req_id, "ok": ok })
}

/// The `answer` reply of one query.
fn answer_json<Item, Error>(
	op: &str,
	req_id: u64,
	answer: Result<Item, Unanswered<Error>>,
	ok: impl FnOnce(&Item) -> Value,
	domain: impl FnOnce(&Error) -> Value,
) -> Value {
	match answer {
		Ok(item) => answer_ok(op, req_id, ok(&item)),
		Err(unanswered) => {
			let error = match unanswered {
				Unanswered::Refused(CallError::Domain(error)) => domain(&error),
				Unanswered::Refused(CallError::Denied) => json!({ "tag": "Denied" }),
				Unanswered::Refused(CallError::Unsupported) => json!({ "tag": "Unsupported" }),
				Unanswered::Refused(CallError::Cancelled) => json!({ "tag": "Cancelled" }),
				Unanswered::Refused(
					CallError::MalformedFrame { reason } | CallError::HostFailure { reason },
				) => json!({ "tag": "HostFailure", "reason": reason }),
				Unanswered::Timeout => json!({ "tag": "Timeout" }),
				Unanswered::NoWorker => json!({ "tag": "NoWorker" }),
				Unanswered::Closed(reason) => json!({ "tag": "Closed", "reason": reason }),
			};
			json!({ "kind": "answer", "op": op, "reqId": req_id, "error": error })
		},
	}
}

fn query_error_json(error: &ProductChatQueryError) -> Value {
	match error {
		ProductChatQueryError::UnknownRoom => json!({ "tag": "UnknownRoom" }),
		ProductChatQueryError::NotAvailable => json!({ "tag": "NotAvailable" }),
		ProductChatQueryError::Unknown { reason } => json!({ "tag": "Unknown", "reason": reason }),
	}
}

fn user_action_error_json(error: &ProductChatUserActionError) -> Value {
	match error {
		ProductChatUserActionError::UnknownRoom => json!({ "tag": "UnknownRoom" }),
		ProductChatUserActionError::UnknownMessage => json!({ "tag": "UnknownMessage" }),
		ProductChatUserActionError::Rejected { reason } => {
			json!({ "tag": "Rejected", "reason": reason })
		},
		ProductChatUserActionError::Unknown { reason } => {
			json!({ "tag": "Unknown", "reason": reason })
		},
	}
}

fn page_json(page: &ProductChatMessagesQueryItem) -> Value {
	json!({
		"messages": page.messages.iter().map(message_json).collect::<Vec<_>>(),
		"next": page.next,
	})
}

fn reactions_json(item: &ProductChatReactionsQueryItem) -> Value {
	json!({
		"reactions": item.reactions.iter().map(|reaction| json!({
			"participantId": reaction.participant_id,
			"emoji": reaction.emoji,
		})).collect::<Vec<_>>(),
	})
}

fn user_action_json(item: &ProductChatUserActionItem) -> Value {
	match item {
		ProductChatUserActionItem::MessageCreated(message) => {
			json!({ "type": "MessageCreated", "message": message_json(message) })
		},
		ProductChatUserActionItem::Acknowledged => json!({ "type": "Acknowledged" }),
	}
}

fn participants_json(item: &ProductChatParticipantsQueryItem) -> Value {
	let entries =
		|entries: &[ChatParticipantEntry]| entries.iter().map(participant_json).collect::<Vec<_>>();
	match item {
		ProductChatParticipantsQueryItem::Count { count } => {
			json!({ "type": "Count", "count": count })
		},
		ProductChatParticipantsQueryItem::Page { participants, next } => {
			json!({ "type": "Page", "participants": entries(participants), "next": next })
		},
		ProductChatParticipantsQueryItem::Resolved { participants } => {
			json!({ "type": "Resolved", "participants": entries(participants) })
		},
	}
}

/// The surface view of one participant, with what each kind draws from.
fn participant_json(entry: &ChatParticipantEntry) -> Value {
	let hex32 = |bytes: &[u8; 32]| format!("0x{}", hex::encode(bytes));
	let participant = match &entry.participant {
		ChatParticipant::Person { person_id } => json!({ "kind": "Person", "personId": person_id }),
		ChatParticipant::LitePerson { account } => {
			json!({ "kind": "LitePerson", "account": hex32(account) })
		},
		ChatParticipant::Contact { handle } => {
			json!({ "kind": "Contact", "handle": hex32(&handle.bytes) })
		},
		ChatParticipant::Product { product_id } => {
			json!({ "kind": "Product", "productId": product_id })
		},
		ChatParticipant::Custom { nickname, avatar } => json!({
			"kind": "Custom",
			"nickname": nickname,
			"avatar": avatar.as_ref().map(resource_json),
		}),
	};
	json!({ "participantId": entry.participant_id, "participant": participant })
}

fn product_action_json(action: &ChatProductAction) -> Value {
	json!({
		"actionId": action.action_id,
		"label": action.label,
		"icon": action.icon.as_ref().map(resource_json),
		"destructive": action.destructive,
		"handler": handler_json(&action.handler),
	})
}

fn built_in_name(action: &ChatBuiltInMessageAction) -> &'static str {
	match action {
		ChatBuiltInMessageAction::Copy => "Copy",
		ChatBuiltInMessageAction::Reply => "Reply",
		ChatBuiltInMessageAction::Edit => "Edit",
		ChatBuiltInMessageAction::Delete => "Delete",
		ChatBuiltInMessageAction::React => "React",
		ChatBuiltInMessageAction::Retry => "Retry",
	}
}

fn message_menu_json(menu: &ChatMessageMenu) -> Value {
	match menu {
		ChatMessageMenu::Custom { actions } => json!({
			"type": "Custom",
			"actions": actions.iter().map(product_action_json).collect::<Vec<_>>(),
		}),
		ChatMessageMenu::ExtendBuiltIn { actions, hidden } => json!({
			"type": "ExtendBuiltIn",
			"actions": actions.iter().map(product_action_json).collect::<Vec<_>>(),
			"hidden": hidden.iter().map(built_in_name).collect::<Vec<_>>(),
		}),
	}
}

fn participant_menu_json(menu: &ChatParticipantMenu) -> Value {
	let (kind, actions) = match menu {
		ChatParticipantMenu::Custom { actions } => ("Custom", actions),
		ChatParticipantMenu::ExtendBuiltIn { actions } => ("ExtendBuiltIn", actions),
	};
	json!({
		"type": kind,
		"actions": actions.iter().map(product_action_json).collect::<Vec<_>>(),
	})
}

#[cfg(test)]
mod tests {
	use std::collections::VecDeque;

	use super::*;
	use truapi::{
		latest::{
			ChatActionHandler, ChatCommandSpec, ChatFeedUpdate, ChatMessage, ChatMessageOrigin,
			ChatMessageStatus, ChatReactionDetail, ChatRoomConfig, ChatRoomMetadata,
			HostChatCreateRoomRequest, HostChatSetRoomCommandsRequest, HostChatUpdateFeedRequest,
		},
		platform::{ChatPlatform, ProductContext},
	};

	type Scripted<Item, Error = ProductChatQueryError> =
		Mutex<VecDeque<Result<Item, CallError<Error>>>>;

	/// Records what a worker connection was handed, and answers each query
	/// from a script. A query with no scripted answer never answers.
	#[derive(Default)]
	struct FakeWorker {
		received: Mutex<Vec<HostChatActionSubscribeItem>>,
		asked: Mutex<Vec<String>>,
		pages: Scripted<ProductChatMessagesQueryItem>,
		actions: Scripted<ProductChatUserActionItem, ProductChatUserActionError>,
		reactions: Scripted<ProductChatReactionsQueryItem>,
		participants: Scripted<ProductChatParticipantsQueryItem>,
		message_menus: Scripted<ProductChatMessageActionsQueryItem>,
		participant_menus: Scripted<ProductChatParticipantActionsQueryItem>,
		room_events: Mutex<Vec<HostChatRoomEventsSubscribeItem>>,
	}

	fn scripted<Item: Send + 'static, Error: Send + 'static>(
		queue: &Scripted<Item, Error>,
	) -> Query<Item, Error> {
		match queue.lock().expect("not poisoned").pop_front() {
			Some(answer) => Subscription::new(futures::stream::once(async move { answer })),
			None => Subscription::new(futures::stream::pending()),
		}
	}

	impl FakeWorker {
		fn received(&self) -> Vec<HostChatActionSubscribeItem> {
			self.received.lock().expect("not poisoned").clone()
		}

		fn events(&self) -> Vec<ChatEvent> {
			self.received().into_iter().map(|item| item.event).collect()
		}

		fn asked(&self) -> Vec<String> {
			self.asked.lock().expect("not poisoned").clone()
		}

		fn ask(&self, request: impl std::fmt::Debug) {
			self.asked.lock().expect("not poisoned").push(format!("{request:?}"));
		}

		fn script<Item, Error>(
			queue: &Scripted<Item, Error>,
			answer: Result<Item, CallError<Error>>,
		) {
			queue.lock().expect("not poisoned").push_back(answer);
		}
	}

	impl ChatWorker for FakeWorker {
		fn publish_chat_event(
			&self,
			item: HostChatActionSubscribeItem,
		) -> Result<(), ProductRuntimeError> {
			self.received.lock().expect("not poisoned").push(item);
			Ok(())
		}

		fn messages_query(
			&self,
			request: ProductChatMessagesQueryRequest,
		) -> Result<Query<ProductChatMessagesQueryItem>, ProductRuntimeError> {
			self.ask(request);
			Ok(scripted(&self.pages))
		}

		fn user_action(
			&self,
			request: ProductChatUserActionRequest,
		) -> Result<Query<ProductChatUserActionItem, ProductChatUserActionError>, ProductRuntimeError>
		{
			self.ask(request);
			Ok(scripted(&self.actions))
		}

		fn reactions_query(
			&self,
			request: ProductChatReactionsQueryRequest,
		) -> Result<Query<ProductChatReactionsQueryItem>, ProductRuntimeError> {
			self.ask(request);
			Ok(scripted(&self.reactions))
		}

		fn participants_query(
			&self,
			request: ProductChatParticipantsQueryRequest,
		) -> Result<Query<ProductChatParticipantsQueryItem>, ProductRuntimeError> {
			self.ask(request);
			Ok(scripted(&self.participants))
		}

		fn message_actions_query(
			&self,
			request: ProductChatMessageActionsQueryRequest,
		) -> Result<Query<ProductChatMessageActionsQueryItem>, ProductRuntimeError> {
			self.ask(request);
			Ok(scripted(&self.message_menus))
		}

		fn participant_actions_query(
			&self,
			request: ProductChatParticipantActionsQueryRequest,
		) -> Result<Query<ProductChatParticipantActionsQueryItem>, ProductRuntimeError> {
			self.ask(request);
			Ok(scripted(&self.participant_menus))
		}

		fn publish_chat_room_event(
			&self,
			item: HostChatRoomEventsSubscribeItem,
		) -> Result<(), ProductRuntimeError> {
			self.room_events.lock().expect("not poisoned").push(item);
			Ok(())
		}
	}

	/// A worker connection the core refuses Chat to.
	struct DeniedWorker;

	impl ChatWorker for DeniedWorker {
		fn publish_chat_event(
			&self,
			_item: HostChatActionSubscribeItem,
		) -> Result<(), ProductRuntimeError> {
			Err(ProductRuntimeError::Denied)
		}

		fn messages_query(
			&self,
			_request: ProductChatMessagesQueryRequest,
		) -> Result<Query<ProductChatMessagesQueryItem>, ProductRuntimeError> {
			Err(ProductRuntimeError::Denied)
		}

		fn user_action(
			&self,
			_request: ProductChatUserActionRequest,
		) -> Result<Query<ProductChatUserActionItem, ProductChatUserActionError>, ProductRuntimeError>
		{
			Err(ProductRuntimeError::Denied)
		}

		fn reactions_query(
			&self,
			_request: ProductChatReactionsQueryRequest,
		) -> Result<Query<ProductChatReactionsQueryItem>, ProductRuntimeError> {
			Err(ProductRuntimeError::Denied)
		}

		fn participants_query(
			&self,
			_request: ProductChatParticipantsQueryRequest,
		) -> Result<Query<ProductChatParticipantsQueryItem>, ProductRuntimeError> {
			Err(ProductRuntimeError::Denied)
		}

		fn message_actions_query(
			&self,
			_request: ProductChatMessageActionsQueryRequest,
		) -> Result<Query<ProductChatMessageActionsQueryItem>, ProductRuntimeError> {
			Err(ProductRuntimeError::Denied)
		}

		fn participant_actions_query(
			&self,
			_request: ProductChatParticipantActionsQueryRequest,
		) -> Result<Query<ProductChatParticipantActionsQueryItem>, ProductRuntimeError> {
			Err(ProductRuntimeError::Denied)
		}

		fn publish_chat_room_event(
			&self,
			_item: HostChatRoomEventsSubscribeItem,
		) -> Result<(), ProductRuntimeError> {
			Err(ProductRuntimeError::Denied)
		}
	}

	const SHORT: Timeouts = Timeouts {
		page: Duration::from_millis(300),
		send: Duration::from_millis(300),
		action: Duration::from_millis(300),
		query: Duration::from_millis(300),
		retry: Duration::from_millis(20),
	};

	fn product() -> ProductContext {
		ProductContext::new("chat.dot".to_string()).expect("valid product id")
	}

	/// A surface over the room `daily` the host keeps and the room `kept` the
	/// product keeps and serves every query of.
	fn surface(worker_bundle: Option<PathBuf>) -> Arc<ChatSurface> {
		let chat = CliChatHost::new(None);
		let create = |room_id: &str, name: &str, config: ChatRoomConfig| {
			futures::executor::block_on(chat.create_chat_room(
				&product(),
				HostChatCreateRoomRequest {
					room_id: room_id.to_string(),
					metadata: ChatRoomMetadata {
						name: name.to_string(),
						avatar: None,
						description: None,
					},
					config,
					preview: None,
				},
				String::new(),
			))
			.expect("a new room is created");
		};
		create("daily", "Daily", ChatRoomConfig::default());
		create(
			"kept",
			"Kept",
			ChatRoomConfig {
				product_source: true,
				reaction_details: true,
				participants: true,
				message_actions: true,
				participant_actions: true,
				report_seen: true,
				allowed_reactions: Some(vec!["+1".to_string()]),
				placeholder: None,
			},
		);
		ChatSurface::with_timeouts(Arc::clone(&chat), worker_bundle, "chat.dot".to_string(), SHORT)
	}

	fn attach(surface: &Arc<ChatSurface>) -> (Arc<FakeWorker>, WorkerAttachment) {
		let worker = Arc::new(FakeWorker::default());
		let sink: Arc<dyn ChatWorker> = worker.clone();
		let attachment = surface.attach_worker(&sink);
		(worker, attachment)
	}

	/// The reply to one operation. An operation that needs the product waits
	/// for its answer.
	async fn run(surface: &Arc<ChatSurface>, op: Value) -> Value {
		match surface.apply(&op.to_string()) {
			Applied::Now(reply) => reply,
			Applied::Later(answer) => answer.await,
		}
	}

	fn now(surface: &Arc<ChatSurface>, op: Value) -> Value {
		match surface.apply(&op.to_string()) {
			Applied::Now(reply) => reply,
			Applied::Later(_) => panic!("{op} waits for the product"),
		}
	}

	fn product_message(message_id: &str, origin: &str) -> ChatMessage {
		ChatMessage {
			message_id: message_id.to_string(),
			origin: ChatMessageOrigin { participant_id: origin.to_string() },
			timestamp: 1,
			body: ChatMessageBody::Text { text: message_id.to_string() },
			status: ChatMessageStatus::Sent,
			reply_to: None,
			edited_at: None,
			reactions: Vec::new(),
		}
	}

	fn room_event(event: ChatEvent) -> HostChatActionSubscribeItem {
		HostChatActionSubscribeItem { room_id: "daily".to_string(), peer: PEER.to_string(), event }
	}

	#[test]
	fn the_page_is_offered_only_for_tcp_endpoints() {
		assert_eq!(page_url("ws://127.0.0.1:9955").as_deref(), Some("http://127.0.0.1:9955/chat"));
		assert_eq!(page_url("ws+unix:/tmp/truapi/frames.sock"), None);
	}

	#[test]
	fn page_ops_parse_by_their_op_tag() {
		let parse = |text: &str| serde_json::from_str::<PageOp>(text).ok();
		assert_eq!(
			parse(r#"{"op":"post","reqId":3,"roomId":"r","text":" hi ","replyTo":"m1"}"#),
			Some(PageOp::Post {
				req_id: 3,
				room_id: "r".to_string(),
				text: " hi ".to_string(),
				reply_to: Some("m1".to_string()),
			})
		);
		assert_eq!(
			parse(r#"{"op":"post","roomId":"r","text":"hi"}"#),
			Some(PageOp::Post {
				req_id: 0,
				room_id: "r".to_string(),
				text: "hi".to_string(),
				reply_to: None,
			})
		);
		assert_eq!(
			parse(r#"{"op":"trigger","roomId":"r","messageId":"m1","actionId":"claim"}"#),
			Some(PageOp::Trigger {
				room_id: "r".to_string(),
				message_id: "m1".to_string(),
				action_id: "claim".to_string(),
			})
		);
		assert_eq!(
			parse(
				r#"{"op":"react","reqId":1,"roomId":"r","messageId":"m","emoji":"+1","placed":false}"#
			),
			Some(PageOp::React {
				req_id: 1,
				room_id: "r".to_string(),
				message_id: "m".to_string(),
				emoji: "+1".to_string(),
				placed: false,
			})
		);
		assert_eq!(
			parse(
				r#"{"op":"messages","reqId":2,"roomId":"r","before":true,"cursor":null,"pageSize":40}"#
			),
			Some(PageOp::Messages {
				req_id: 2,
				room_id: "r".to_string(),
				before: true,
				cursor: None,
				page_size: 40,
			})
		);
		assert_eq!(
			parse(
				r#"{"op":"participants","reqId":4,"roomId":"r","query":{"type":"Page","cursor":"100","pageSize":100}}"#
			),
			Some(PageOp::Participants {
				req_id: 4,
				room_id: "r".to_string(),
				query: ParticipantsOp::Page { cursor: Some("100".to_string()), page_size: 100 },
			})
		);
		assert_eq!(
			parse(
				r#"{"op":"participants","reqId":5,"roomId":"r","query":{"type":"ByIds","participantIds":["p1"]}}"#
			),
			Some(PageOp::Participants {
				req_id: 5,
				room_id: "r".to_string(),
				query: ParticipantsOp::ByIds { participant_ids: vec!["p1".to_string()] },
			})
		);
		assert_eq!(
			parse(r#"{"op":"participantAction","roomId":"r","participantId":"p1","actionId":"a"}"#),
			Some(PageOp::ParticipantAction {
				room_id: "r".to_string(),
				participant_id: "p1".to_string(),
				action_id: "a".to_string(),
			})
		);
		assert_eq!(
			parse(r#"{"op":"open","roomId":"r"}"#),
			Some(PageOp::Open { room_id: "r".to_string() })
		);
		assert!(parse(r#"{"op":"post","roomId":"r"}"#).is_none());
		assert!(parse(r#"{"op":"messages","roomId":"r","before":true,"pageSize":40}"#).is_none());
		assert!(parse(r#"{"op":"nope","roomId":"r"}"#).is_none());
	}

	/// A person's line in a room the host keeps is stored with a host id and
	/// reaches every worker as `MessagePosted` with that id.
	#[test]
	fn a_post_is_stored_and_reaches_every_worker_with_its_id() {
		let surface = surface(None);
		let (first, _first) = attach(&surface);
		let (second, _second) = attach(&surface);
		let (_, mut events) = surface.chat().observe();

		let reply = now(
			&surface,
			json!({"op":"post","reqId":7,"roomId":"daily","text":" gm\n","replyTo":"m0"}),
		);

		assert_eq!(reply["kind"], "answer");
		assert_eq!(reply["reqId"], 7);
		assert_eq!(reply["ok"]["workers"], 2);
		let message_id = reply["ok"]["message"]["messageId"].as_str().expect("an id").to_string();
		let expected = room_event(ChatEvent::MessagePosted {
			message_id: message_id.clone(),
			body: ChatMessageBody::Text { text: " gm\n".to_string() },
			reply_to: Some("m0".to_string()),
		});
		assert_eq!(first.received(), vec![expected.clone()]);
		assert_eq!(second.received(), vec![expected]);
		let message = events.try_recv().expect("the message event");
		assert_eq!(message["message"]["origin"], "user");
		assert_eq!(message["message"]["messageId"], message_id.as_str());
		assert_eq!(message["message"]["replyTo"], "m0");
	}

	#[test]
	fn a_detached_worker_receives_nothing_more() {
		let surface = surface(None);
		let (kept, _kept) = attach(&surface);
		let (gone, attachment) = attach(&surface);
		drop(attachment);

		let reply =
			now(&surface, json!({"op":"trigger","roomId":"daily","messageId":"m1","actionId":"a"}));

		assert_eq!(reply["workers"], 1);
		assert_eq!(kept.received().len(), 1);
		assert!(gone.received().is_empty());
		assert_eq!(surface.connected_workers(), 1);
	}

	#[test]
	fn every_event_of_the_screen_carries_what_the_page_sent() {
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		for op in [
			json!({"op":"trigger","roomId":"daily","messageId":"m4","actionId":"claim"}),
			json!({"op":"messageAction","roomId":"daily","messageId":"m4","actionId":"act-pin"}),
			json!({"op":"participantAction","roomId":"daily","participantId":"p1","actionId":"act-mention"}),
			json!({"op":"toolbar","roomId":"daily","actionId":"tb-2"}),
			json!({"op":"suggestion","roomId":"daily","suggestionId":"sg-event"}),
			json!({"op":"open","roomId":"daily"}),
			json!({"op":"close","roomId":"daily"}),
		] {
			assert_eq!(now(&surface, op.clone())["kind"], "delivered", "{op}");
		}
		assert_eq!(
			worker.events(),
			vec![
				ChatEvent::ActionTriggered(ActionTrigger {
					message_id: "m4".to_string(),
					action_id: "claim".to_string(),
					payload: None,
				}),
				ChatEvent::MessageActionTriggered {
					message_id: "m4".to_string(),
					action_id: "act-pin".to_string(),
				},
				ChatEvent::ParticipantActionTriggered {
					participant_id: "p1".to_string(),
					action_id: "act-mention".to_string(),
				},
				ChatEvent::ToolbarActionTriggered { action_id: "tb-2".to_string() },
				ChatEvent::SuggestionSelected { suggestion_id: "sg-event".to_string() },
				ChatEvent::RoomOpened,
				ChatEvent::RoomClosed,
			]
		);
		// None of them is a message, so none takes a message id.
		assert!(surface.chat().observe().0.messages.is_empty());
	}

	/// Only a name the room declared after `/` is a command.
	#[test]
	fn a_command_needs_a_declared_name() {
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		let command =
			json!({"op":"command","roomId":"daily","command":"send","payload":"5 to bob"});

		assert_eq!(now(&surface, command.clone())["kind"], "error");
		futures::executor::block_on(surface.chat().set_chat_room_commands(
			&product(),
			HostChatSetRoomCommandsRequest {
				room_id: "daily".to_string(),
				commands: vec![ChatCommandSpec {
					name: "send".to_string(),
					description: "Send".to_string(),
					argument_hint: Some("<amount> to <who>".to_string()),
				}],
			},
		))
		.expect("the room exists");
		assert_eq!(now(&surface, command)["kind"], "delivered");

		assert_eq!(
			worker.events(),
			vec![ChatEvent::Command(ChatCommand {
				command: "send".to_string(),
				payload: "5 to bob".to_string(),
			})]
		);
	}

	#[test]
	fn the_users_changes_in_a_room_the_host_keeps_become_events() {
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		let posted = now(&surface, json!({"op":"post","reqId":1,"roomId":"daily","text":"hi"}));
		let id = posted["ok"]["message"]["messageId"].as_str().expect("an id").to_string();

		let edit = now(
			&surface,
			json!({"op":"edit","reqId":2,"roomId":"daily","messageId":id,"text":"hello"}),
		);
		let react = now(
			&surface,
			json!({"op":"react","reqId":3,"roomId":"daily","messageId":id,"emoji":"+1","placed":true}),
		);
		let seen = now(&surface, json!({"op":"seen","reqId":4,"roomId":"daily","messageId":id}));
		let delete =
			now(&surface, json!({"op":"delete","reqId":5,"roomId":"daily","messageId":id}));

		assert_eq!((edit["kind"].clone(), edit["reqId"].clone()), (json!("delivered"), json!(2)));
		assert_eq!(react["reqId"], 3);
		// The room does not ask for seen reports, so the host only keeps it.
		assert_eq!(seen, json!({"kind":"answer","op":"seen","reqId":4,"ok":{"type":"Kept"}}));
		assert_eq!(delete["reqId"], 5);
		assert_eq!(
			worker.events()[1..],
			[
				ChatEvent::UserEdited {
					message_id: id.clone(),
					body: ChatMessageBody::Text { text: "hello".to_string() },
				},
				ChatEvent::UserReacted {
					message_id: id.clone(),
					emoji: "+1".to_string(),
					placed: true
				},
				ChatEvent::UserDeleted { message_id: id.clone() },
			]
		);
		// A refusal names the request it refuses.
		let refused = now(
			&surface,
			json!({"op":"edit","reqId":6,"roomId":"daily","messageId":"nope","text":"x"}),
		);
		assert_eq!(refused["kind"], "error");
		assert_eq!(refused["reqId"], 6);
		let retry = now(&surface, json!({"op":"retry","reqId":8,"roomId":"daily","messageId":id}));
		assert_eq!(retry["kind"], "error", "retry belongs to rooms the product keeps");
	}

	#[tokio::test]
	async fn a_send_in_a_room_the_product_keeps_waits_for_the_product() {
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		let (_, mut events) = surface.chat().observe();
		let created = product_message("kept-u-1", "user");
		FakeWorker::script(
			&worker.actions,
			Ok(ProductChatUserActionItem::MessageCreated(created.clone())),
		);

		let reply =
			run(&surface, json!({"op":"post","reqId":9,"roomId":"kept","text":"hello"})).await;

		assert_eq!(reply["kind"], "answer");
		assert_eq!(reply["reqId"], 9);
		assert_eq!(reply["ok"]["type"], "MessageCreated");
		assert_eq!(reply["ok"]["message"]["messageId"], "kept-u-1");
		assert_eq!(
			worker.asked(),
			vec![format!(
				"{:?}",
				ProductChatUserActionRequest {
					room_id: "kept".to_string(),
					action: ChatUserAction::NewMessage {
						body: ChatMessageBody::Text { text: "hello".to_string() },
						reply_to: None,
					},
				}
			)]
		);
		assert!(worker.received().is_empty(), "no event: the product heard it as a user action");
		let shown = events.try_recv().expect("the created message is drawn");
		assert_eq!(shown["message"]["messageId"], "kept-u-1");
		assert!(surface.chat().message("kept", "kept-u-1").is_some());
	}

	#[tokio::test]
	async fn a_rejected_or_unanswered_send_says_why() {
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		FakeWorker::script(
			&worker.actions,
			Err(CallError::Domain(ProductChatUserActionError::Rejected {
				reason: "Rejected by /fail".to_string(),
			})),
		);

		let rejected =
			run(&surface, json!({"op":"post","reqId":1,"roomId":"kept","text":"a"})).await;
		let unanswered =
			run(&surface, json!({"op":"post","reqId":2,"roomId":"kept","text":"b"})).await;

		assert_eq!(rejected["error"], json!({"tag":"Rejected","reason":"Rejected by /fail"}));
		assert_eq!(unanswered["error"], json!({"tag":"Timeout"}));
		assert_eq!(unanswered["reqId"], 2);
	}

	#[tokio::test]
	async fn an_unavailable_query_is_asked_once_more() {
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		FakeWorker::script(&worker.pages, Err(CallError::unavailable()));
		FakeWorker::script(
			&worker.pages,
			Ok(ProductChatMessagesQueryItem {
				messages: vec![product_message("h-1", "product")],
				next: Some("0".to_string()),
			}),
		);

		let page = run(
			&surface,
			json!({"op":"messages","reqId":3,"roomId":"kept","before":true,"pageSize":40}),
		)
		.await;

		assert_eq!(worker.asked().len(), 2);
		assert_eq!(page["ok"]["next"], "0");
		assert_eq!(page["ok"]["messages"][0]["messageId"], "h-1");
		// The page is in the cache, so a menu can be asked for it.
		assert!(surface.chat().message("kept", "h-1").is_some());

		FakeWorker::script(&worker.pages, Err(CallError::unavailable()));
		FakeWorker::script(&worker.pages, Err(CallError::unavailable()));
		let refused = run(
			&surface,
			json!({"op":"messages","reqId":4,"roomId":"kept","before":false,"cursor":"5","pageSize":40}),
		)
		.await;
		assert_eq!(refused["error"], json!({"tag":"HostFailure","reason":"unavailable"}));
		assert!(worker.asked()[2].contains("After"), "{:?}", worker.asked());
	}

	#[tokio::test]
	async fn a_query_with_no_worker_waits_for_one_then_gives_up() {
		let surface = surface(None);
		let missing =
			run(&surface, json!({"op":"reactions","reqId":1,"roomId":"kept","messageId":"m"}))
				.await;
		assert_eq!(missing["error"], json!({"tag":"NoWorker"}));

		// A worker that attaches while the page waits answers.
		let waiting = surface.apply(
			&json!({"op":"reactions","reqId":2,"roomId":"kept","messageId":"m"}).to_string(),
		);
		let (worker, _attachment) = attach(&surface);
		FakeWorker::script(
			&worker.reactions,
			Ok(ProductChatReactionsQueryItem {
				reactions: vec![ChatReactionDetail {
					participant_id: "p1".to_string(),
					emoji: "+1".to_string(),
				}],
			}),
		);
		let Applied::Later(answer) = waiting else { panic!("a query waits") };
		assert_eq!(answer.await["ok"], json!({"reactions":[{"participantId":"p1","emoji":"+1"}]}));

		let denied: Arc<dyn ChatWorker> = Arc::new(DeniedWorker);
		let _denied = surface.attach_worker(&denied);
		let closed =
			run(&surface, json!({"op":"reactions","reqId":3,"roomId":"kept","messageId":"m"}))
				.await;
		assert_eq!(closed["error"]["tag"], "Closed");
	}

	#[tokio::test]
	async fn participants_resolve_and_name_the_menu_the_page_asks() {
		use truapi::latest::ChatParticipant;
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		FakeWorker::script(
			&worker.participants,
			Ok(ProductChatParticipantsQueryItem::Resolved {
				participants: vec![ChatParticipantEntry {
					participant_id: "p3".to_string(),
					participant: ChatParticipant::Contact {
						handle: truapi::latest::ContactHandle { bytes: [3; 32] },
					},
				}],
			}),
		);
		FakeWorker::script(
			&worker.participant_menus,
			Ok(ProductChatParticipantActionsQueryItem {
				menu: ChatParticipantMenu::ExtendBuiltIn {
					actions: vec![ChatProductAction {
						action_id: "act-mention".to_string(),
						label: "Mention".to_string(),
						icon: None,
						destructive: false,
						handler: ChatActionHandler::Callback,
					}],
				},
			}),
		);

		let unknown = now(
			&surface,
			json!({"op":"participantActions","reqId":1,"roomId":"kept","participantId":"p3"}),
		);
		assert_eq!(unknown["kind"], "error", "a participant the product never named");
		let resolved = run(&surface, json!({"op":"participants","reqId":2,"roomId":"kept","query":{"type":"ByIds","participantIds":["user","p3","product"]}})).await;
		assert_eq!(resolved["ok"]["type"], "Resolved");
		assert_eq!(
			resolved["ok"]["participants"][0],
			json!({
				"participantId": "p3",
				"participant": { "kind": "Contact", "handle": format!("0x{}", "03".repeat(32)) },
			})
		);
		// The reserved ids resolve without a question.
		assert!(worker.asked()[0].contains(r#"participant_ids: ["p3"]"#), "{:?}", worker.asked());

		let menu = run(
			&surface,
			json!({"op":"participantActions","reqId":3,"roomId":"kept","participantId":"p3"}),
		)
		.await;
		assert_eq!(
			menu["ok"],
			json!({
				"type": "ExtendBuiltIn",
				"actions": [{
					"actionId": "act-mention",
					"label": "Mention",
					"icon": null,
					"destructive": false,
					"handler": { "type": "Callback" },
				}],
			})
		);
	}

	#[tokio::test]
	async fn a_message_menu_is_asked_with_the_message_the_host_holds() {
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		futures::executor::block_on(surface.chat().update_chat_feed(
			&product(),
			HostChatUpdateFeedRequest {
				room_id: "daily".to_string(),
				update: ChatFeedUpdate::New(product_message("p-1", "product")),
			},
			true,
		))
		.expect("stored");
		FakeWorker::script(
			&worker.message_menus,
			Ok(ProductChatMessageActionsQueryItem {
				menu: ChatMessageMenu::ExtendBuiltIn {
					actions: vec![ChatProductAction {
						action_id: "act-report".to_string(),
						label: "Report".to_string(),
						icon: None,
						destructive: true,
						handler: ChatActionHandler::Deeplink { url: "polkadot://x".to_string() },
					}],
					hidden: vec![ChatBuiltInMessageAction::Edit],
				},
			}),
		);

		let menu = run(
			&surface,
			json!({"op":"messageActions","reqId":1,"roomId":"daily","messageId":"p-1"}),
		)
		.await;

		assert_eq!(menu["ok"]["type"], "ExtendBuiltIn");
		assert_eq!(menu["ok"]["hidden"], json!(["Edit"]));
		assert_eq!(menu["ok"]["actions"][0]["destructive"], true);
		assert_eq!(
			menu["ok"]["actions"][0]["handler"],
			json!({"type":"Deeplink","url":"polkadot://x"})
		);
		assert!(worker.asked()[0].contains(r#"message_id: "p-1""#));
		let missing = now(
			&surface,
			json!({"op":"messageActions","reqId":2,"roomId":"daily","messageId":"nope"}),
		);
		assert_eq!((missing["kind"].clone(), missing["reqId"].clone()), (json!("error"), json!(2)));
	}

	#[tokio::test]
	async fn the_users_changes_in_a_room_the_product_keeps_are_user_actions() {
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		surface.chat().cache_page("kept", &[product_message("s-1", "user")]);
		for _ in 0..4 {
			FakeWorker::script(&worker.actions, Ok(ProductChatUserActionItem::Acknowledged));
		}

		let react = run(
			&surface,
			json!({"op":"react","reqId":1,"roomId":"kept","messageId":"s-1","emoji":"+1","placed":true}),
		)
		.await;
		let seen =
			run(&surface, json!({"op":"seen","reqId":2,"roomId":"kept","messageId":"s-1"})).await;
		let retry =
			run(&surface, json!({"op":"retry","reqId":3,"roomId":"kept","messageId":"s-1"})).await;
		let edit = run(
			&surface,
			json!({"op":"edit","reqId":4,"roomId":"kept","messageId":"s-1","text":"new"}),
		)
		.await;
		let refused = now(
			&surface,
			json!({"op":"react","reqId":5,"roomId":"kept","messageId":"s-1","emoji":"-1","placed":true}),
		);

		for reply in [&react, &seen, &retry, &edit] {
			assert_eq!(reply["ok"], json!({"type":"Acknowledged"}), "{reply}");
		}
		assert_eq!(refused["kind"], "error", "the room offers only +1");
		let actions: Vec<_> = worker.asked();
		assert_eq!(actions.len(), 4);
		assert!(actions[0].contains("ReactionPlaced"));
		assert!(actions[1].contains("Seen"));
		assert!(actions[2].contains("Retry"));
		assert!(actions[3].contains("EditMessage"));
		let cached = surface.chat().message("kept", "s-1").expect("cached");
		assert_eq!(cached.body, ChatMessageBody::Text { text: "new".to_string() });
		assert!(cached.reactions[0].reacted_by_user);
		assert!(worker.received().is_empty());
	}

	/// Over the socket: the snapshot first, then the answer of a query the
	/// page asked, matched by its request id, while events still flow.
	#[tokio::test]
	async fn the_socket_answers_a_query_with_its_request_id() -> Result<()> {
		use tokio_tungstenite::tungstenite::protocol::Role;
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		FakeWorker::script(
			&worker.pages,
			Ok(ProductChatMessagesQueryItem {
				messages: vec![product_message("h-9", "product")],
				next: None,
			}),
		);
		let (page_end, host_end) = tokio::io::duplex(1 << 16);
		let served = Arc::clone(&surface);
		let server = tokio::spawn(async move {
			let ws = WebSocketStream::from_raw_socket(host_end, Role::Server, None).await;
			served.serve_ws(ws).await
		});
		let mut page = WebSocketStream::from_raw_socket(page_end, Role::Client, None).await;
		let Some(Ok(Message::Text(first))) = page.next().await else {
			panic!("the surface speaks first");
		};
		assert_eq!(serde_json::from_str::<Value>(&first)?["kind"], "snapshot");

		let ask = json!({"op":"messages","reqId":41,"roomId":"kept","before":true,"pageSize":40});
		page.send(Message::Text(ask.to_string())).await?;
		page.send(Message::Text(json!({"op":"open","roomId":"kept"}).to_string()))
			.await?;
		let mut frames = Vec::new();
		while frames.len() < 2 {
			match page.next().await {
				Some(Ok(Message::Text(text))) => frames.push(serde_json::from_str::<Value>(&text)?),
				other => panic!("expected a frame, got {other:?}"),
			}
		}
		let answer = frames.iter().find(|frame| frame["kind"] == "answer").expect("the answer");
		assert_eq!(answer["reqId"], 41);
		assert_eq!(answer["ok"]["messages"][0]["messageId"], "h-9");
		assert!(frames.iter().any(|frame| frame["kind"] == "delivered" && frame["op"] == "open"));
		assert_eq!(worker.events(), vec![ChatEvent::RoomOpened]);

		page.close(None).await?;
		tokio::time::timeout(Duration::from_secs(1), server).await???;
		Ok(())
	}

	#[test]
	fn an_unknown_room_is_refused_before_any_worker_sees_it() {
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);

		for op in [
			json!({"op":"post","roomId":"nope","text":"hi"}),
			json!({"op":"trigger","roomId":"nope","messageId":"m1","actionId":"a"}),
			json!({"op":"command","roomId":"nope","command":"c","payload":""}),
			json!({"op":"messages","reqId":1,"roomId":"nope","before":true,"pageSize":40}),
		] {
			let reply = now(&surface, op.clone());
			assert_eq!(reply["kind"], "error", "{op}");
			assert!(reply["message"].as_str().unwrap_or_default().contains("nope"), "{reply}");
		}
		assert!(worker.received().is_empty());
		assert!(worker.asked().is_empty());
		assert!(surface.chat().observe().0.messages.is_empty());
		assert_eq!(now(&surface, json!("{"))["kind"], "error");
	}

	/// Zero tells the page nobody is listening, which is what a person needs
	/// to know when a reply never comes.
	#[test]
	fn delivered_counts_only_live_workers() {
		let surface = surface(None);
		let reply = now(&surface, json!({"op":"open","roomId":"daily"}));
		assert_eq!(reply, json!({"kind": "delivered", "op": "open", "workers": 0}));

		let denied: Arc<dyn ChatWorker> = Arc::new(DeniedWorker);
		let _denied = surface.attach_worker(&denied);
		let reply = now(&surface, json!({"op":"open","roomId":"daily"}));
		assert_eq!(reply["kind"], "error");
		assert_eq!(reply["message"], ProductRuntimeError::Denied.to_string().as_str());
	}

	#[test]
	fn the_snapshot_names_the_time_outs_the_page_waits_for() {
		let surface = surface(None);
		let (snapshot, _) = surface.chat().observe();
		let view = surface.snapshot(snapshot, 0);
		assert_eq!(view["timeouts"], json!({"page":300,"send":300,"action":300,"query":300}));
		assert_eq!(view["rooms"].as_array().map(Vec::len), Some(2));
		let defaults = Timeouts::default();
		assert_eq!(
			(defaults.page, defaults.send, defaults.action, defaults.query, defaults.retry),
			(
				Duration::from_secs(10),
				Duration::from_secs(30),
				Duration::from_secs(10),
				Duration::from_secs(5),
				Duration::from_secs(1),
			)
		);
	}

	#[test]
	fn the_page_boots_the_worker_only_when_a_bundle_is_configured() {
		let without = surface(None).page();
		assert!(!without.contains(WORKER_BOOT_MARKER));
		assert!(!without.contains(WORKER_BUNDLE_PATH));

		let with = surface(Some(PathBuf::from("/tmp/index.js"))).page();
		assert!(!with.contains(WORKER_BOOT_MARKER));
		assert!(with.contains(r#"<script src="/worker/bootstrap.js"></script>"#));
		assert!(with.contains(r#"<script type="module" src="/worker/index.js"></script>"#));
	}

	#[test]
	fn the_worker_bundle_is_read_from_disk_on_every_request() -> Result<()> {
		assert!(surface(None).worker_bundle().is_err());

		let bundle = tempfile::NamedTempFile::new()?;
		let surface = surface(Some(bundle.path().to_path_buf()));
		std::fs::write(bundle.path(), "first")?;
		assert_eq!(surface.worker_bundle().as_deref(), Ok("first"));
		std::fs::write(bundle.path(), "second")?;
		assert_eq!(surface.worker_bundle().as_deref(), Ok("second"));
		Ok(())
	}

	// S49: the page shows the features the host lists, as a product reads
	// them from `capabilities`.
	#[test]
	fn the_snapshot_lists_the_capabilities_of_the_page() {
		let surface = surface(None);
		let (snapshot, _) = surface.chat().observe();

		assert_eq!(
			surface.snapshot(snapshot, 0)["capabilities"],
			json!({
				"features": crate::chat::CLI_CHAT_FEATURES,
				"inlineAttachmentMaxBytes": 1024 * 1024,
				"preimageAttachmentMaxBytes": 0,
			})
		);
	}

	#[test]
	fn the_user_state_of_a_room_reaches_the_product() {
		let surface = surface(None);
		let op = json!({"op":"userState","roomId":"daily","mutedUntil":5,"archived":true});

		// Without a worker the state is kept and the page hears why the
		// product did not.
		assert_eq!(now(&surface, op.clone())["kind"], "error");
		assert_eq!(surface.chat().user_states().len(), 1);

		let (worker, _attachment) = attach(&surface);
		assert_eq!(now(&surface, op)["kind"], "delivered");
		// A page cannot write `u64::MAX` as a JSON number, so it asks for a
		// mute with no end by name.
		let forever =
			json!({"op":"userState","roomId":"daily","mutedForever":true,"archived":false});
		assert_eq!(now(&surface, forever)["kind"], "delivered");
		assert_eq!(surface.chat().user_states()[0].muted_until, Some(u64::MAX));
		assert_eq!(
			*worker.room_events.lock().expect("not poisoned"),
			[
				HostChatRoomEventsSubscribeItem {
					room_id: "daily".to_string(),
					event: ChatRoomEvent::UserStateChanged { muted_until: Some(5), archived: true },
				},
				HostChatRoomEventsSubscribeItem {
					room_id: "daily".to_string(),
					event: ChatRoomEvent::UserStateChanged {
						muted_until: Some(u64::MAX),
						archived: false,
					},
				},
			]
		);
	}
}
