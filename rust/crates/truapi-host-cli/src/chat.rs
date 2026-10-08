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

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::channel::mpsc;
use futures::stream::{self, BoxStream};
use parity_scale_codec::Encode;
use truapi::latest::{
    ChatBotRegistrationStatus, ChatMessageContent, ChatRoomRegistrationStatus, GenericError,
    HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
    HostChatListSubscribeItem, HostChatPostMessageError, HostChatPostMessageRequest,
    HostChatPostMessageResponse, HostChatRegisterBotError, HostChatRegisterBotRequest,
    HostChatRegisterBotResponse,
};
use truapi::platform::{ChatPlatform, ProductContext, async_trait};
use truapi::v01::{ChatRoom, ChatRoomParticipation};

/// Rooms, bots and posted messages for one process.
#[derive(Default)]
struct State {
    /// Room id to how this host participates in it.
    rooms: BTreeMap<String, ChatRoomParticipation>,
    /// Registered bot ids. A bot is not a room, so registering one does not
    /// republish the room list.
    bots: BTreeSet<String>,
    /// Messages accepted so far. The count is what the next message id counts
    /// from, and a product correlates an action trigger against that id.
    accepted: usize,
    /// Live room-list subscribers, one per product connection.
    subscribers: Vec<mpsc::UnboundedSender<HostChatListSubscribeItem>>,
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
    fn new(transcript: Option<PathBuf>) -> Arc<Self> {
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

    /// Current room list, in room-id order so a replacement that changes
    /// nothing is byte-identical to the one before it.
    fn room_list(state: &State) -> HostChatListSubscribeItem {
        HostChatListSubscribeItem {
            rooms: state
                .rooms
                .iter()
                .map(|(room_id, participating_as)| ChatRoom {
                    room_id: room_id.clone(),
                    participating_as: *participating_as,
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

    /// Append one accepted message to the transcript, if one is configured.
    fn record_message(&self, message_id: &str, request: &HostChatPostMessageRequest) {
        self.record(serde_json::json!({
            "kind": "message",
            "messageId": message_id,
            "roomId": request.room_id,
            "variant": variant_name(&request.payload),
            // The payload as the host received it. A summary would let a
            // difference between what a product sent and what a host stored
            // hide behind the summary.
            "payload": hex::encode(request.payload.encode()),
        }));
    }

    /// Append one accepted room or bot registration.
    fn record_registration(&self, kind: &str, id: &str, name: &str, icon: &str) {
        self.record(serde_json::json!({
            "kind": kind,
            "id": id,
            "name": name,
            // Icons resolve before a host sees them, so record what arrived
            // rather than what the product typed.
            "icon": icon,
        }));
    }

    /// Append one line to the transcript, if one is configured.
    fn record(&self, line: serde_json::Value) {
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
            state
                .rooms
                .insert(request.room_id.clone(), ChatRoomParticipation::RoomHost);
            Self::republish(&mut state);
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
        let mut state = self.lock();
        if !state.rooms.contains_key(&request.room_id) {
            // A room this host never created is not one it can store against.
            return Err(HostChatPostMessageError::Unknown {
                reason: format!("unknown room {:?}", request.room_id),
            });
        }
        state.accepted += 1;
        let message_id = format!("m{}", state.accepted);
        drop(state);
        self.record_message(&message_id, &request);
        Ok(HostChatPostMessageResponse { message_id })
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
    use parity_scale_codec::OptionBool;
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
            hide_text_input: OptionBool(None),
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
}
