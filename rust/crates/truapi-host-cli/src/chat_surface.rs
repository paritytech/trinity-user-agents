//! Browser surface for the chat a development worker holds.
//!
//! `truapi-host dev` stands in for the phone a chat product runs beside. The
//! worker connects on [`WORKER_PATH`] and talks to the in-memory
//! [`CliChatHost`]; this module serves a page on [`PAGE_PATH`] that draws what
//! the worker posted and lets a person answer, press an action or send a
//! command. Those arrive at every live worker through `chat.actionSubscribe()`,
//! the way a phone delivers them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::watch;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use truapi::latest::ChatMessageContent;
use truapi::v01::{ActionTrigger, ChatActionPayload, ChatCommand, HostChatActionSubscribeItem};
use truapi::{ProductRuntime, ProductRuntimeError};

use crate::chat::{ChatSnapshot, CliChatHost};

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

/// Somewhere a person's chat action can be delivered to a product.
pub trait ChatActionSink: Send + Sync {
    /// Queue `item` for the product's `chat.actionSubscribe()`.
    fn publish_chat_action(
        &self,
        item: HostChatActionSubscribeItem,
    ) -> Result<(), ProductRuntimeError>;
}

impl ChatActionSink for ProductRuntime {
    fn publish_chat_action(
        &self,
        item: HostChatActionSubscribeItem,
    ) -> Result<(), ProductRuntimeError> {
        self.control().publish_chat_action(item)
    }
}

/// Live worker connections, keyed by attachment order.
#[derive(Default)]
struct Workers {
    next: u64,
    live: BTreeMap<u64, Weak<dyn ChatActionSink>>,
}

/// The development chat surface for one host.
pub struct ChatSurface {
    chat: Arc<CliChatHost>,
    worker_bundle: Option<PathBuf>,
    product_id: String,
    workers: Mutex<Workers>,
    /// Number of live workers, for every open page.
    connected: watch::Sender<usize>,
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

/// An operation a page sends, tagged on `op`.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "camelCase")]
enum PageOp {
    #[serde(rename_all = "camelCase")]
    Post { room_id: String, text: String },
    #[serde(rename_all = "camelCase")]
    Trigger {
        room_id: String,
        message_id: String,
        action_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Command {
        room_id: String,
        command: String,
        payload: String,
    },
}

impl ChatSurface {
    /// Build a surface over `chat` for `product_id`, serving the worker bundle
    /// at `worker_bundle` when one is configured.
    pub fn new(
        chat: Arc<CliChatHost>,
        worker_bundle: Option<PathBuf>,
        product_id: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            chat,
            worker_bundle,
            product_id,
            workers: Mutex::new(Workers::default()),
            connected: watch::channel(0).0,
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
        self.workers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Deliver the surface's actions to `worker` until the returned
    /// attachment is dropped.
    pub fn attach_worker(self: &Arc<Self>, worker: &Arc<dyn ChatActionSink>) -> WorkerAttachment {
        let mut workers = self.lock_workers();
        let id = workers.next;
        workers.next += 1;
        workers.live.insert(id, Arc::downgrade(worker));
        self.connected.send_replace(workers.live.len());
        WorkerAttachment {
            surface: Arc::clone(self),
            id,
        }
    }

    /// The page, booting the worker from it only when a bundle is configured.
    pub fn page(&self) -> String {
        let boot = if self.worker_configured() {
            WORKER_BOOT
        } else {
            ""
        };
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

    /// Drive one page connection: the snapshot, then every chat and worker
    /// change, answering each operation the page sends.
    pub async fn serve_ws<S>(&self, ws: WebSocketStream<S>) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let (snapshot, mut events) = self.chat.observe();
        let mut connected = self.connected.subscribe();
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
                message = read.next() => match message {
                    Some(Ok(Message::Text(text))) => self.apply(&text),
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
            "rooms": snapshot.rooms,
            "bots": snapshot.bots,
            "messages": snapshot.messages,
        })
    }

    /// Apply one page operation, returning the `delivered` or `error` reply.
    fn apply(&self, text: &str) -> Value {
        let op = match serde_json::from_str::<PageOp>(text) {
            Ok(op) => op,
            Err(error) => return error_reply(format!("malformed op: {error}")),
        };
        let (name, room_id, payload) = match op {
            PageOp::Post { room_id, text } => (
                "post",
                room_id,
                ChatActionPayload::MessagePosted(ChatMessageContent::Text { text }),
            ),
            PageOp::Trigger {
                room_id,
                message_id,
                action_id,
            } => (
                "trigger",
                room_id,
                ChatActionPayload::ActionTriggered(ActionTrigger {
                    message_id,
                    action_id,
                    payload: None,
                }),
            ),
            PageOp::Command {
                room_id,
                command,
                payload,
            } => (
                "command",
                room_id,
                ChatActionPayload::Command(ChatCommand { command, payload }),
            ),
        };
        if let ChatActionPayload::MessagePosted(content) = &payload {
            // Recorded first, so the person's message precedes any reply the
            // worker posts to it.
            if self
                .chat
                .post_person_message(&room_id, content.clone())
                .is_none()
            {
                return unknown_room(&room_id);
            }
        } else if !self.chat.has_room(&room_id) {
            return unknown_room(&room_id);
        }
        let item = HostChatActionSubscribeItem {
            room_id,
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
        let workers: Vec<_> = self
            .lock_workers()
            .live
            .values()
            .filter_map(Weak::upgrade)
            .collect();
        let mut accepted = 0;
        let mut refusal = None;
        for worker in workers {
            match worker.publish_chat_action(item.clone()) {
                Ok(()) => accepted += 1,
                Err(error) => {
                    refusal.get_or_insert(error);
                }
            }
        }
        refusal.map_or(Ok(accepted), Err)
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

#[cfg(test)]
mod tests {
    use super::*;
    use truapi::latest::HostChatCreateRoomRequest;
    use truapi::platform::{ChatPlatform, ProductContext};

    /// Records what a worker connection was handed.
    #[derive(Default)]
    struct FakeWorker {
        received: Mutex<Vec<HostChatActionSubscribeItem>>,
    }

    impl ChatActionSink for FakeWorker {
        fn publish_chat_action(
            &self,
            item: HostChatActionSubscribeItem,
        ) -> Result<(), ProductRuntimeError> {
            self.received.lock().expect("not poisoned").push(item);
            Ok(())
        }
    }

    impl FakeWorker {
        fn received(&self) -> Vec<HostChatActionSubscribeItem> {
            self.received.lock().expect("not poisoned").clone()
        }
    }

    /// A worker connection the core refuses Chat to.
    struct DeniedWorker;

    impl ChatActionSink for DeniedWorker {
        fn publish_chat_action(
            &self,
            _item: HostChatActionSubscribeItem,
        ) -> Result<(), ProductRuntimeError> {
            Err(ProductRuntimeError::Denied)
        }
    }

    fn surface(worker_bundle: Option<PathBuf>) -> Arc<ChatSurface> {
        let chat = CliChatHost::new(None);
        let product = ProductContext::new("chat.dot".to_string()).expect("valid product id");
        futures::executor::block_on(chat.create_chat_room(
            &product,
            HostChatCreateRoomRequest {
                room_id: "daily".to_string(),
                name: "Daily".to_string(),
                icon: String::new(),
            },
        ))
        .expect("a new room is created");
        ChatSurface::new(chat, worker_bundle, "chat.dot".to_string())
    }

    fn attach(surface: &Arc<ChatSurface>) -> (Arc<FakeWorker>, WorkerAttachment) {
        let worker = Arc::new(FakeWorker::default());
        let sink: Arc<dyn ChatActionSink> = worker.clone();
        let attachment = surface.attach_worker(&sink);
        (worker, attachment)
    }

    #[test]
    fn the_page_is_offered_only_for_tcp_endpoints() {
        assert_eq!(
            page_url("ws://127.0.0.1:9955").as_deref(),
            Some("http://127.0.0.1:9955/chat")
        );
        assert_eq!(page_url("ws+unix:/tmp/truapi/frames.sock"), None);
    }

    #[test]
    fn page_ops_parse_by_their_op_tag() {
        assert_eq!(
            serde_json::from_str::<PageOp>(r#"{"op":"post","roomId":"r","text":" hi "}"#).ok(),
            Some(PageOp::Post {
                room_id: "r".to_string(),
                text: " hi ".to_string(),
            })
        );
        assert_eq!(
            serde_json::from_str::<PageOp>(
                r#"{"op":"trigger","roomId":"r","messageId":"m1","actionId":"claim"}"#
            )
            .ok(),
            Some(PageOp::Trigger {
                room_id: "r".to_string(),
                message_id: "m1".to_string(),
                action_id: "claim".to_string(),
            })
        );
        assert_eq!(
            serde_json::from_str::<PageOp>(
                r#"{"op":"command","roomId":"r","command":"help","payload":""}"#
            )
            .ok(),
            Some(PageOp::Command {
                room_id: "r".to_string(),
                command: "help".to_string(),
                payload: String::new(),
            })
        );
        assert!(serde_json::from_str::<PageOp>(r#"{"op":"post","roomId":"r"}"#).is_err());
    }

    /// A person's line reaches every worker as `MessagePosted`, and every
    /// open page shows it as that person's message.
    #[test]
    fn a_post_reaches_every_worker_and_is_recorded_as_the_persons() {
        let surface = surface(None);
        let (first, _first) = attach(&surface);
        let (second, _second) = attach(&surface);
        let (_, mut events) = surface.chat().observe();

        let reply = surface.apply(r#"{"op":"post","roomId":"daily","text":" gm\n"}"#);

        assert_eq!(
            reply,
            json!({"kind": "delivered", "op": "post", "workers": 2})
        );
        let expected = HostChatActionSubscribeItem {
            room_id: "daily".to_string(),
            peer: "native".to_string(),
            payload: ChatActionPayload::MessagePosted(ChatMessageContent::Text {
                text: " gm\n".to_string(),
            }),
        };
        assert_eq!(first.received(), vec![expected.clone()]);
        assert_eq!(second.received(), vec![expected]);
        let message = events.try_recv().expect("the message event");
        assert_eq!(message["author"], "person");
        assert_eq!(message["content"], json!({"type": "Text", "text": " gm\n"}));
    }

    #[test]
    fn a_detached_worker_receives_nothing_more() {
        let surface = surface(None);
        let (kept, _kept) = attach(&surface);
        let (gone, attachment) = attach(&surface);
        drop(attachment);

        let reply =
            surface.apply(r#"{"op":"command","roomId":"daily","command":"x","payload":""}"#);

        assert_eq!(reply["workers"], 1);
        assert_eq!(kept.received().len(), 1);
        assert!(gone.received().is_empty());
        assert_eq!(surface.connected_workers(), 1);
    }

    #[test]
    fn triggers_and_commands_carry_what_the_page_sent() {
        let surface = surface(None);
        let (worker, _attachment) = attach(&surface);

        surface.apply(r#"{"op":"trigger","roomId":"daily","messageId":"m4","actionId":"claim"}"#);
        surface.apply(r#"{"op":"command","roomId":"daily","command":"send","payload":"5 to bob"}"#);

        let payloads: Vec<_> = worker
            .received()
            .into_iter()
            .map(|item| item.payload)
            .collect();
        assert_eq!(
            payloads,
            vec![
                ChatActionPayload::ActionTriggered(ActionTrigger {
                    message_id: "m4".to_string(),
                    action_id: "claim".to_string(),
                    payload: None,
                }),
                ChatActionPayload::Command(ChatCommand {
                    command: "send".to_string(),
                    payload: "5 to bob".to_string(),
                }),
            ]
        );
        // Neither is a message, so neither takes a message id.
        assert!(surface.chat().observe().0.messages.is_empty());
    }

    #[test]
    fn an_unknown_room_is_refused_before_any_worker_sees_it() {
        let surface = surface(None);
        let (worker, _attachment) = attach(&surface);

        for op in [
            r#"{"op":"post","roomId":"nope","text":"hi"}"#,
            r#"{"op":"trigger","roomId":"nope","messageId":"m1","actionId":"a"}"#,
            r#"{"op":"command","roomId":"nope","command":"c","payload":""}"#,
        ] {
            let reply = surface.apply(op);
            assert_eq!(reply["kind"], "error", "{op}");
            assert!(
                reply["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("nope"),
                "{reply}"
            );
        }
        assert!(worker.received().is_empty());
        assert!(surface.chat().observe().0.messages.is_empty());
        assert_eq!(surface.apply("{")["kind"], "error");
    }

    /// Zero tells the page nobody is listening, which is what a person needs
    /// to know when a reply never comes.
    #[test]
    fn delivered_counts_only_live_workers() {
        let surface = surface(None);
        let reply = surface.apply(r#"{"op":"post","roomId":"daily","text":"hi"}"#);
        assert_eq!(
            reply,
            json!({"kind": "delivered", "op": "post", "workers": 0})
        );

        let denied: Arc<dyn ChatActionSink> = Arc::new(DeniedWorker);
        let _denied = surface.attach_worker(&denied);
        let reply = surface.apply(r#"{"op":"post","roomId":"daily","text":"hi"}"#);
        assert_eq!(reply["kind"], "error");
        assert_eq!(
            reply["message"],
            ProductRuntimeError::Denied.to_string().as_str()
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
