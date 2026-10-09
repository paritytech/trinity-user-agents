//! In-memory Chat host for the CLI.
//!
//! Rooms, bots and messages live for the length of the process: this exists to
//! make a chat product runnable headlessly, not to be a chat backend.
//!
//! Every message the core hands over is appended to the transcript named by
//! `TRUAPI_CHAT_LOG`, one JSON object per line. A product-visible error alone
//! cannot tell "the core rejected this before any host saw it" apart from "the
//! host was handed it and refused", and that distinction is the whole point of
//! screening content in the runtime; the transcript is what lets a battery
//! assert the first reading.
//!
//! The same state is what the development chat surface draws, so a host also
//! keeps each room's name and icon, each bot, and every accepted message, and
//! streams changes to any observer as the JSON the surface protocol names.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::channel::mpsc;
use futures::stream::{self, BoxStream};
use parity_scale_codec::Encode;
use serde_json::{Value, json};
use truapi::latest::{
    ChatBotRegistrationStatus, ChatMessageContent, ChatRoomRegistrationStatus, GenericError,
    HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
    HostChatListSubscribeItem, HostChatPostMessageError, HostChatPostMessageRequest,
    HostChatPostMessageResponse, HostChatRegisterBotError, HostChatRegisterBotRequest,
    HostChatRegisterBotResponse, HostChatSetRoomFooterRequest,
};
use truapi::platform::{ChatPlatform, ProductContext, async_trait};
use truapi::v01::{ChatActionLayout, ChatRoom, ChatRoomParticipation};

/// A room this host created, as the product described it.
struct RoomRecord {
    participating_as: ChatRoomParticipation,
    name: String,
    icon: String,
}

/// A bot the product registered. A bot is not a room, so registering one does
/// not republish the room list.
struct BotRecord {
    name: String,
    icon: String,
}

/// Who wrote a message this host accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Author {
    /// The product posted it through `Chat::post_message`.
    Product,
    /// A person posted it from the development chat surface.
    Person,
}

impl Author {
    fn as_str(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Person => "person",
        }
    }
}

/// One message this host accepted, from either side of the room.
struct MessageRecord {
    message_id: String,
    room_id: String,
    author: Author,
    content: ChatMessageContent,
}

/// Rooms, bots and posted messages for one process.
#[derive(Default)]
struct State {
    /// Room id to what the product created it as.
    rooms: BTreeMap<String, RoomRecord>,
    /// Bot id to what the product registered it as.
    bots: BTreeMap<String, BotRecord>,
    /// Every message accepted so far, product and person alike. The count is
    /// what the next message id counts from, and a product correlates an
    /// action trigger against that id, so both authors share one sequence.
    messages: Vec<MessageRecord>,
    /// Live room-list subscribers, one per product connection.
    subscribers: Vec<mpsc::UnboundedSender<HostChatListSubscribeItem>>,
    /// Live surface observers, each receiving `room`/`bot`/`message` events.
    observers: Vec<mpsc::UnboundedSender<Value>>,
}

/// Everything an observer is shown on subscribing, as surface-protocol JSON.
pub struct ChatSnapshot {
    /// `Room` objects in room-id order.
    pub rooms: Vec<Value>,
    /// `Bot` objects in bot-id order.
    pub bots: Vec<Value>,
    /// `Message` objects in the order this host accepted them.
    pub messages: Vec<Value>,
}

/// A chat host that keeps everything in memory.
pub struct CliChatHost {
    state: Mutex<State>,
    transcript: Option<PathBuf>,
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
        if let Some(path) = transcript.as_ref()
            && let Err(error) = std::fs::write(path, b"")
        {
            tracing::warn!(?path, %error, "chat transcript could not be truncated");
        }
        Arc::new(Self {
            state: Mutex::new(State::default()),
            transcript,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Current state plus every change after it. Both are taken under one
    /// lock, so an observer neither misses nor repeats a change that lands
    /// while it subscribes.
    pub fn observe(&self) -> (ChatSnapshot, mpsc::UnboundedReceiver<Value>) {
        let mut state = self.lock();
        let snapshot = ChatSnapshot {
            rooms: state
                .rooms
                .iter()
                .map(|(room_id, room)| room_json(room_id, room))
                .collect(),
            bots: state
                .bots
                .iter()
                .map(|(bot_id, bot)| bot_json(bot_id, bot))
                .collect(),
            messages: state.messages.iter().map(message_json).collect(),
        };
        let (sender, receiver) = mpsc::unbounded();
        state.observers.push(sender);
        (snapshot, receiver)
    }

    /// Whether the product has created `room_id` on this host.
    pub fn has_room(&self, room_id: &str) -> bool {
        self.lock().rooms.contains_key(room_id)
    }

    /// Accept a message a person posted into `room_id`, returning the id it
    /// was given, or `None` when the product never created that room.
    pub fn post_person_message(
        &self,
        room_id: &str,
        content: ChatMessageContent,
    ) -> Option<String> {
        self.accept(room_id, Author::Person, content)
    }

    /// Store one message under the next id and tell every observer, then
    /// append it to the transcript.
    fn accept(&self, room_id: &str, author: Author, content: ChatMessageContent) -> Option<String> {
        let mut state = self.lock();
        if !state.rooms.contains_key(room_id) {
            return None;
        }
        let message_id = format!("m{}", state.messages.len() + 1);
        let message = MessageRecord {
            message_id: message_id.clone(),
            room_id: room_id.to_string(),
            author,
            content,
        };
        let event = with_kind("message", message_json(&message));
        state.messages.push(message);
        Self::broadcast(&mut state, event);
        let message = state.messages.last().expect("the message was just stored");
        let line = transcript_message(message);
        drop(state);
        self.record(line);
        Some(message_id)
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

/// The transcript line for one accepted message.
fn transcript_message(message: &MessageRecord) -> Value {
    json!({
        "kind": "message",
        "messageId": message.message_id,
        "roomId": message.room_id,
        "author": message.author.as_str(),
        "variant": variant_name(&message.content),
        // The payload as the host received it. A summary would let a
        // difference between what a product sent and what a host stored
        // hide behind the summary.
        "payload": hex::encode(message.content.encode()),
    })
}

/// `body` with the surface-protocol `kind` discriminant added.
fn with_kind(kind: &str, mut body: Value) -> Value {
    body["kind"] = Value::from(kind);
    body
}

fn room_json(room_id: &str, room: &RoomRecord) -> Value {
    json!({
        "roomId": room_id,
        "name": room.name,
        "icon": room.icon,
        "participatingAs": match room.participating_as {
            ChatRoomParticipation::RoomHost => "RoomHost",
            ChatRoomParticipation::Bot => "Bot",
        },
    })
}

fn bot_json(bot_id: &str, bot: &BotRecord) -> Value {
    json!({ "botId": bot_id, "name": bot.name, "icon": bot.icon })
}

fn message_json(message: &MessageRecord) -> Value {
    json!({
        "messageId": message.message_id,
        "roomId": message.room_id,
        "author": message.author.as_str(),
        "content": content_json(&message.content),
    })
}

/// The surface-protocol `Content` view of a message. Every field is kept, and
/// a custom payload travels as hex, so the surface draws what the host holds.
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
            "layout": match actions.layout {
                ChatActionLayout::Column => "Column",
                ChatActionLayout::Grid => "Grid",
            },
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

#[async_trait]
impl ChatPlatform for CliChatHost {
    async fn create_chat_room(
        &self,
        _product: &ProductContext,
        request: HostChatCreateRoomRequest,
    ) -> Result<HostChatCreateRoomResponse, HostChatCreateRoomError> {
        let mut state = self.lock();
        let status = if state.rooms.contains_key(&request.room_id) {
            ChatRoomRegistrationStatus::Exists
        } else {
            let room = RoomRecord {
                participating_as: ChatRoomParticipation::RoomHost,
                name: request.name.clone(),
                icon: request.icon.clone(),
            };
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
        let status = if state.bots.contains_key(&request.bot_id) {
            ChatBotRegistrationStatus::Exists
        } else {
            let bot = BotRecord {
                name: request.name.clone(),
                icon: request.icon.clone(),
            };
            let event = with_kind("bot", bot_json(&request.bot_id, &bot));
            state.bots.insert(request.bot_id.clone(), bot);
            Self::broadcast(&mut state, event);
            ChatBotRegistrationStatus::New
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
        let message_id = self
            .accept(&request.room_id, Author::Product, request.payload)
            .ok_or_else(|| HostChatPostMessageError::Unknown {
                reason: format!("unknown room {:?}", request.room_id),
            })?;
        Ok(HostChatPostMessageResponse { message_id })
    }

    async fn set_chat_room_footer(
        &self,
        _product: &ProductContext,
        _request: HostChatSetRoomFooterRequest,
    ) -> Result<(), GenericError> {
        Ok(())
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
        stream::once(async move { Ok(snapshot) })
            .chain(receiver.map(Ok))
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::read_to_string;

    fn product() -> ProductContext {
        ProductContext::new("chat.dot".to_string()).expect("valid product id")
    }

    fn text(text: &str) -> ChatMessageContent {
        ChatMessageContent::Text {
            text: text.to_string(),
        }
    }

    fn room(room_id: &str) -> HostChatCreateRoomRequest {
        HostChatCreateRoomRequest {
            room_id: room_id.to_string(),
            name: "Support".to_string(),
            icon: String::new(),
        }
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
            },
        ));

        assert!(matches!(
            posted,
            Err(HostChatPostMessageError::Unknown { .. })
        ));
        // A refused message is not one this host was willing to store, so it
        // must not appear in the record a battery reads.
        assert_eq!(
            read_to_string(transcript.path()).expect("the transcript is readable"),
            ""
        );
    }

    #[test]
    fn a_stored_message_is_recorded_as_the_host_received_it() {
        let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
        let host = CliChatHost::new(Some(transcript.path().to_path_buf()));
        futures::executor::block_on(host.create_chat_room(&product(), room("support")))
            .expect("a new room is created");

        let payload = text("line one\nline two");
        let posted = futures::executor::block_on(host.post_chat_message(
            &product(),
            HostChatPostMessageRequest {
                room_id: "support".to_string(),
                payload: payload.clone(),
            },
        ))
        .expect("a message posts into a room this host created");

        assert_eq!(posted.message_id, "m1");
        let transcript_text =
            read_to_string(transcript.path()).expect("the transcript is readable");
        let recorded: serde_json::Value = serde_json::from_str(
            transcript_text
                .lines()
                .next_back()
                .expect("the message is the last line, after the room"),
        )
        .expect("each line is one JSON object");
        assert_eq!(recorded["kind"], "message");
        assert_eq!(recorded["messageId"], "m1");
        assert_eq!(recorded["roomId"], "support");
        assert_eq!(recorded["variant"], "Text");
        // The payload as bytes, so a difference between what a product sent
        // and what the host received cannot hide behind a rendering.
        assert_eq!(recorded["payload"], hex::encode(payload.encode()));
    }

    #[test]
    fn a_room_appears_in_the_list_a_subscriber_already_holds() {
        let host = CliChatHost::new(None);
        let mut rooms = host.subscribe_chat_rooms(&product());

        let snapshot = futures::executor::block_on(rooms.next())
            .expect("a subscription emits its snapshot")
            .expect("the snapshot is not an error");
        assert!(snapshot.rooms.is_empty());

        futures::executor::block_on(host.create_chat_room(&product(), room("support")))
            .expect("a new room is created");

        let replacement = futures::executor::block_on(rooms.next())
            .expect("creating a room republishes the list")
            .expect("the replacement is not an error");
        assert_eq!(replacement.rooms.len(), 1);
        assert_eq!(replacement.rooms[0].room_id, "support");
    }

    fn post(host: &CliChatHost, room_id: &str, content: ChatMessageContent) -> String {
        futures::executor::block_on(host.post_chat_message(
            &product(),
            HostChatPostMessageRequest {
                room_id: room_id.to_string(),
                payload: content,
            },
        ))
        .expect("the room exists")
        .message_id
    }

    /// A surface that opens before the worker creates its room must still
    /// learn about the room, and one that opens after must not be told twice.
    #[test]
    fn an_observer_sees_changes_after_its_snapshot_exactly_once() {
        let host = CliChatHost::new(None);
        let (before, mut early) = host.observe();
        assert!(before.rooms.is_empty());

        futures::executor::block_on(host.create_chat_room(&product(), room("support")))
            .expect("a new room is created");
        let (after, mut late) = host.observe();
        let id = post(&host, "support", text("hi"));

        let room = early.try_recv().expect("the room event");
        assert_eq!(
            room,
            json!({
                "kind": "room",
                "roomId": "support",
                "name": "Support",
                "icon": "",
                "participatingAs": "RoomHost",
            })
        );
        let message = early.try_recv().expect("the message event");
        assert_eq!(message["kind"], "message");
        assert_eq!(message["messageId"], id);

        assert_eq!(after.rooms, vec![with_kind_removed(room)]);
        let only = late.try_recv().expect("the message event");
        assert_eq!(only, message);
        assert!(late.try_recv().is_err(), "the room was in the snapshot");
    }

    fn with_kind_removed(mut event: Value) -> Value {
        event
            .as_object_mut()
            .expect("an event is an object")
            .remove("kind");
        event
    }

    /// A product correlates an action trigger by message id, so an id the
    /// person's message took must never be handed to a product message too.
    #[test]
    fn person_and_product_messages_share_one_id_sequence() {
        let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
        let host = CliChatHost::new(Some(transcript.path().to_path_buf()));
        futures::executor::block_on(host.create_chat_room(&product(), room("support")))
            .expect("a new room is created");

        let first = post(&host, "support", text("welcome"));
        let second = host
            .post_person_message("support", text("hello"))
            .expect("the room exists");
        let third = post(&host, "support", text("hi there"));
        assert_eq!(
            (first.as_str(), second.as_str(), third.as_str()),
            ("m1", "m2", "m3")
        );

        let (snapshot, _) = host.observe();
        let authors: Vec<_> = snapshot
            .messages
            .iter()
            .map(|message| message["author"].clone())
            .collect();
        assert_eq!(authors, vec!["product", "person", "product"]);
        let recorded: Vec<Value> = read_to_string(transcript.path())
            .expect("the transcript is readable")
            .lines()
            .skip(1)
            .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
            .collect();
        assert_eq!(recorded[1]["author"], "person");
        assert_eq!(recorded[1]["messageId"], "m2");

        assert_eq!(host.post_person_message("elsewhere", text("lost")), None);
        assert_eq!(host.observe().0.messages.len(), 3);
    }

    #[test]
    fn content_is_shown_with_every_field_the_product_sent() {
        use truapi::v01::{
            ChatAction, ChatActions, ChatCustomMessage, ChatFile, ChatMedia, ChatReaction,
            ChatRichText,
        };
        let reaction = ChatReaction {
            message_id: "m1".to_string(),
            emoji: "+1".to_string(),
        };
        let cases = [
            (text("a\nb"), json!({"type": "Text", "text": "a\nb"})),
            (
                ChatMessageContent::RichText(ChatRichText {
                    text: None,
                    media: vec![ChatMedia {
                        url: "https://x/1.png".to_string(),
                    }],
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
}
