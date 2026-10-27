//! Browser surface for the chat a development worker holds.
//!
//! `truapi-host dev` stands in for the phone a chat product runs beside. The
//! worker connects on [`WORKER_PATH`] and talks to the in-memory
//! [`CliChatHost`]; this module serves a page on [`PAGE_PATH`] that draws the
//! rooms the worker declared and lets a person use them as a phone user does.
//!
//! The page speaks a JSON protocol on [`SOCKET_PATH`]. The host sends a
//! snapshot, then every change to a room or a message. The page sends one
//! operation per thing the person does. A message the person posts into a room
//! the host keeps becomes a Chat action, which reaches every live worker
//! through `chat.actionSubscribe()`. An operation that needs an answer from the
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
		ChatActionPayload, ChatMessageBody, ChatMessageContent, ChatPageAnchor, ChatUserAction,
		HostChatActionSubscribeItem, ProductChatMessagesQueryItem, ProductChatMessagesQueryRequest,
		ProductChatQueryError, ProductChatReactionsQueryItem, ProductChatReactionsQueryRequest,
		ProductChatUserActionError, ProductChatUserActionItem, ProductChatUserActionRequest,
	},
};

use crate::chat::{ChatSnapshot, CliChatHost, message_json};

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
	fn publish_chat_action(
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
}

impl ChatWorker for ProductRuntime {
	fn publish_chat_action(
		&self,
		item: HostChatActionSubscribeItem,
	) -> Result<(), ProductRuntimeError> {
		self.control().publish_chat_action(item)
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
	/// `reactions_query`.
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

/// An operation a page sends, tagged on `op`. `reqId` is the page's own
/// number for the operation, which its `answer` or refusal carries.
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
		/// The id the page chose for the message, in a room the product keeps.
		#[serde(default)]
		message_id: Option<String>,
		/// When the user wrote the message, in milliseconds since the Unix
		/// epoch, in a room the product keeps.
		#[serde(default)]
		timestamp: Option<u64>,
	},
	Edit {
		#[serde(default)]
		req_id: u64,
		room_id: String,
		message_id: String,
		text: String,
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
		let boot = if self.worker_bundle.is_some() { WORKER_BOOT } else { "" };
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
			"rooms": snapshot.rooms,
			"messages": snapshot.messages,
		})
	}

	/// Apply one page operation. A reply at once carries the request id of
	/// the operation, so the page can match a refusal to it.
	fn apply(self: &Arc<Self>, text: &str) -> Applied {
		let op = match serde_json::from_str::<PageOp>(text) {
			Ok(op) => op,
			Err(error) => return Applied::Now(error_reply(format!("malformed op: {error}"))),
		};
		match self.apply_op(&op) {
			Applied::Now(mut reply) if reply.get("reqId").is_none() => {
				reply["reqId"] = Value::from(op_req_id(&op));
				Applied::Now(reply)
			},
			applied => applied,
		}
	}

	fn apply_op(self: &Arc<Self>, op: &PageOp) -> Applied {
		let room_id = op_room(op).to_string();
		let Some(config) = self.chat.room_config(&room_id) else {
			return Applied::Now(unknown_room(&room_id));
		};
		let source = config.product_source;
		let host_kept = || {
			Applied::Now(error_reply(format!(
				"the room {room_id:?} keeps its messages in the host, which takes edits and reactions from the product only"
			)))
		};
		match op {
			PageOp::Post { req_id, text, reply_to, message_id, timestamp, .. } => self.post_text(
				*req_id,
				room_id,
				text.clone(),
				reply_to.clone(),
				(message_id.clone(), *timestamp),
				source,
			),
			PageOp::Edit { req_id, message_id, text, .. } => {
				if !source {
					return host_kept();
				}
				let body = ChatMessageBody::Text { text: text.clone() };
				let action = ChatUserAction::EditMessage { message_id: message_id.clone(), body };
				self.later_action("edit", *req_id, room_id, action)
			},
			PageOp::React { req_id, message_id, emoji, placed, .. } => {
				if !source {
					return host_kept();
				}
				let (message_id, emoji) = (message_id.clone(), emoji.clone());
				let action = if *placed {
					ChatUserAction::ReactionPlaced { message_id, emoji }
				} else {
					ChatUserAction::ReactionRemoved { message_id, emoji }
				};
				self.later_action("react", *req_id, room_id, action)
			},
			PageOp::Seen { req_id, message_id, .. } => {
				if source && config.report_seen {
					let action = ChatUserAction::Seen { message_id: message_id.clone() };
					return self.later_action("seen", *req_id, room_id, action);
				}
				if !source && let Err(reason) = self.chat.mark_seen(&room_id, message_id) {
					return Applied::Now(error_reply(reason));
				}
				Applied::Now(answer_ok("seen", *req_id, json!({ "type": "Kept" })))
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
		}
	}

	/// Send a text of the user: a user action of a room the product keeps,
	/// under the id and with the time the page chose, or a stored message and
	/// `MessagePosted` in a room the host keeps.
	fn post_text(
		self: &Arc<Self>,
		req_id: u64,
		room_id: String,
		text: String,
		reply_to: Option<String>,
		(message_id, timestamp): (Option<String>, Option<u64>),
		source: bool,
	) -> Applied {
		let name = "post";
		if source {
			let body = ChatMessageBody::Text { text };
			let action = ChatUserAction::NewMessage { body, reply_to, message_id, timestamp };
			return self.later_action(name, req_id, room_id, action);
		}
		// Stored first, so the user's message precedes any reply the worker
		// posts to it.
		let body = ChatMessageBody::Text { text: text.clone() };
		match self.chat.post_user_message(&room_id, body, reply_to) {
			Ok(message) => {
				let posted = ChatActionPayload::MessagePosted(ChatMessageContent::Text { text });
				let delivered = self.deliver(name, &room_id, posted);
				if delivered["kind"] == "error" {
					return Applied::Now(delivered);
				}
				Applied::Now(answer_ok(
					name,
					req_id,
					json!({
						"type": "Posted",
						"message": message_json(&message),
						"workers": delivered["workers"],
					}),
				))
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
			ChatUserAction::NewMessage { .. } | ChatUserAction::SendPayment { .. } => {
				self.timeouts.send
			},
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

	/// Deliver one action to every live worker. The answer is the `delivered`
	/// reply or the `error` reply.
	fn deliver(&self, name: &str, room_id: &str, payload: ChatActionPayload) -> Value {
		let item = HostChatActionSubscribeItem {
			room_id: room_id.to_string(),
			peer: PEER.to_string(),
			payload,
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
			match worker.publish_chat_action(item.clone()) {
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
		PageOp::Edit { room_id, .. } |
		PageOp::React { room_id, .. } |
		PageOp::Seen { room_id, .. } |
		PageOp::Messages { room_id, .. } |
		PageOp::Reactions { room_id, .. } => room_id,
	}
}

fn op_req_id(op: &PageOp) -> u64 {
	match op {
		PageOp::Post { req_id, .. } |
		PageOp::Edit { req_id, .. } |
		PageOp::React { req_id, .. } |
		PageOp::Seen { req_id, .. } |
		PageOp::Messages { req_id, .. } |
		PageOp::Reactions { req_id, .. } => *req_id,
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

#[cfg(test)]
mod tests {
	use std::collections::VecDeque;

	use super::*;
	use truapi::{
		latest::{
			ChatMessage, ChatMessageOrigin, ChatMessageStatus, ChatReactionDetail, ChatRoomConfig,
			HostChatCreateRoomRequest, HostChatSetRoomConfigRequest,
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

		fn payloads(&self) -> Vec<ChatActionPayload> {
			self.received().into_iter().map(|item| item.payload).collect()
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
		fn publish_chat_action(
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
	}

	/// A worker connection the core refuses Chat to.
	struct DeniedWorker;

	impl ChatWorker for DeniedWorker {
		fn publish_chat_action(
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
	}

	const SHORT: Timeouts = Timeouts {
		page: Duration::from_millis(300),
		send: Duration::from_millis(300),
		action: Duration::from_millis(300),
		query: Duration::from_millis(300),
		retry: Duration::from_millis(20),
	};

	fn product() -> ProductContext {
		ProductContext::new("chat.dot".parse().expect("valid product id"))
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
					name: name.to_string(),
					icon: String::new(),
				},
			))
			.expect("a new room is created");
			futures::executor::block_on(chat.set_chat_room_config(
				&product(),
				HostChatSetRoomConfigRequest { room_id: room_id.to_string(), config },
			))
			.expect("the room takes its configuration");
		};
		create("daily", "Daily", ChatRoomConfig::default());
		create(
			"kept",
			"Kept",
			ChatRoomConfig { product_source: true, reaction_details: true, report_seen: true },
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
			reply_preview: None,
			edited_at: None,
			reactions: Vec::new(),
			request: false,
		}
	}

	fn posted_text(text: &str) -> ChatActionPayload {
		ChatActionPayload::MessagePosted(ChatMessageContent::Text { text: text.to_string() })
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
				message_id: None,
				timestamp: None,
			})
		);
		assert_eq!(
			parse(r#"{"op":"post","roomId":"r","text":"hi"}"#),
			Some(PageOp::Post {
				req_id: 0,
				room_id: "r".to_string(),
				text: "hi".to_string(),
				reply_to: None,
				message_id: None,
				timestamp: None,
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
		assert!(parse(r#"{"op":"post","roomId":"r"}"#).is_none());
		assert!(parse(r#"{"op":"messages","roomId":"r","before":true,"pageSize":40}"#).is_none());
		assert!(parse(r#"{"op":"nope","roomId":"r"}"#).is_none());
	}

	/// A person's line in a room the host keeps is stored with a host id and
	/// reaches every worker as `MessagePosted`.
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
		assert_eq!(first.payloads(), vec![posted_text(" gm\n")]);
		assert_eq!(second.payloads(), vec![posted_text(" gm\n")]);
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

		let reply = now(&surface, json!({"op":"post","reqId":1,"roomId":"daily","text":"hi"}));

		assert_eq!(reply["ok"]["workers"], 1);
		assert_eq!(kept.received().len(), 1);
		assert!(gone.received().is_empty());
		assert_eq!(surface.connected_workers(), 1);
	}

	#[test]
	fn a_room_the_host_keeps_takes_no_edit_or_reaction_from_the_page() {
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

		// A refusal names the request it refuses.
		assert_eq!((edit["kind"].clone(), edit["reqId"].clone()), (json!("error"), json!(2)));
		assert_eq!((react["kind"].clone(), react["reqId"].clone()), (json!("error"), json!(3)));
		assert_eq!(seen, json!({"kind":"answer","op":"seen","reqId":4,"ok":{"type":"Kept"}}));
		assert_eq!(worker.payloads(), vec![posted_text("hi")]);
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

		let reply = run(
			&surface,
			json!({"op":"post","reqId":9,"roomId":"kept","text":"hello","messageId":"kept-u-1","timestamp":1_767_225_600_000u64}),
		)
		.await;

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
						message_id: Some("kept-u-1".to_string()),
						timestamp: Some(1_767_225_600_000),
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
					timestamp: 1,
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
	async fn the_users_changes_in_a_room_the_product_keeps_are_user_actions() {
		let surface = surface(None);
		let (worker, _attachment) = attach(&surface);
		surface.chat().cache_page("kept", &[product_message("s-1", "user")]);
		for _ in 0..3 {
			FakeWorker::script(&worker.actions, Ok(ProductChatUserActionItem::Acknowledged));
		}

		let react = run(
			&surface,
			json!({"op":"react","reqId":1,"roomId":"kept","messageId":"s-1","emoji":"+1","placed":true}),
		)
		.await;
		let seen =
			run(&surface, json!({"op":"seen","reqId":2,"roomId":"kept","messageId":"s-1"})).await;
		let edit = run(
			&surface,
			json!({"op":"edit","reqId":4,"roomId":"kept","messageId":"s-1","text":"new"}),
		)
		.await;

		for reply in [&react, &seen, &edit] {
			assert_eq!(reply["ok"], json!({"type":"Acknowledged"}), "{reply}");
		}
		let actions: Vec<_> = worker.asked();
		assert_eq!(actions.len(), 3);
		assert!(actions[0].contains("ReactionPlaced"));
		assert!(actions[1].contains("Seen"));
		assert!(actions[2].contains("EditMessage"));
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
		let post = json!({"op":"post","reqId":42,"roomId":"daily","text":"go"});
		page.send(Message::Text(post.to_string())).await?;
		let answer = |frames: &[Value], req_id: u64| {
			frames
				.iter()
				.find(|frame| frame["kind"] == "answer" && frame["reqId"] == req_id)
				.cloned()
		};
		let mut frames = Vec::new();
		while answer(&frames, 41).is_none() || answer(&frames, 42).is_none() {
			match page.next().await {
				Some(Ok(Message::Text(text))) => frames.push(serde_json::from_str::<Value>(&text)?),
				other => panic!("expected a frame, got {other:?}"),
			}
		}
		let page_answer = answer(&frames, 41).expect("the answer");
		assert_eq!(page_answer["ok"]["messages"][0]["messageId"], "h-9");
		assert_eq!(answer(&frames, 42).expect("the answer")["ok"]["workers"], 1);
		assert_eq!(worker.payloads(), vec![posted_text("go")]);

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
			json!({"op":"react","reqId":2,"roomId":"nope","messageId":"m1","emoji":"+1","placed":true}),
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
		let post = || json!({"op":"post","reqId":1,"roomId":"daily","text":"hi"});
		let reply = now(&surface, post());
		assert_eq!(reply["ok"]["workers"], 0);

		let denied: Arc<dyn ChatWorker> = Arc::new(DeniedWorker);
		let _denied = surface.attach_worker(&denied);
		let reply = now(&surface, post());
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
}
