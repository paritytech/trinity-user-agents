//! A counter bot that runs as a Worker product without JavaScript.
//!
//! The guest speaks the TrUAPI wire directly: it builds `ProtocolMessage`
//! envelopes itself and encodes each leg's versioned wrapper with SCALE. It
//! never blocks on the host. Every entry point returns the frames to send, and
//! the embedder relays them; replies come back through [`Worker::on_frame`].
//!
//! Three modes share the counter, the tree and the lifecycle:
//!
//! - [`Mode::Chat`] (the default, what the CLI proof runs): on start it
//!   registers a bot, creates a room, subscribes to renderer actions and posts
//!   one `Custom` message; the host then opens `renderer.render` for that
//!   message.
//! - [`Mode::Pocket`] (the `pocket` cargo feature selects it for the wasm32
//!   ABI): on start it only handshakes and subscribes to renderer actions. It
//!   makes no Chat call, because a host that serves Pocket workers need not
//!   serve Chat, and a refused call would be noise rather than a finding. The
//!   host opens `renderer.render` for a Pocket card.
//! - [`Mode::Unified`] (the `unified` cargo feature): starts as chat mode does,
//!   and draws both its chat message and a Pocket card, so one product shows
//!   the same count on both surfaces.
//!
//! Each `bump` action, from whichever body it was pressed in, increments the
//! one count and redraws every open stream.

use parity_scale_codec::{Decode, DecodeAll, Encode};
use truapi::v01::{
    ButtonProps, ChatCustomMessage, ChatMessageContent, ColumnProps, HostChatCreateRoomRequest,
    HostChatPostMessageRequest, HostChatRegisterBotRequest, HostHandshakeRequest, RenderContext,
    RendererNode, TextProps,
};
use truapi::{CallError, WIRE_CODEC_VERSION, versioned};

#[cfg(target_arch = "wasm32")]
// The sandbox boundary is a C ABI over raw guest memory, which only exists on
// the wasm32 build; every other build keeps the workspace's no-unsafe lint.
#[allow(unsafe_code)]
mod abi;

/// Wire discriminants the guest addresses. Pinned against the generated wire
/// table by the runner's tests; the protocol-only build has no table.
pub mod wire {
    /// `system.handshake`.
    pub const SYSTEM_HANDSHAKE: (u8, u8) = (1, 0);
    /// `chat.createRoom`.
    pub const CHAT_CREATE_ROOM: (u8, u8) = (4, 0);
    /// `chat.registerBot`.
    pub const CHAT_REGISTER_BOT: (u8, u8) = (4, 1);
    /// `chat.postMessage`.
    pub const CHAT_POST_MESSAGE: (u8, u8) = (4, 3);
    /// `renderer.render`, host initiated.
    pub const RENDERER_RENDER: (u8, u8) = (17, 0);
    /// `renderer.actionSubscribe`.
    pub const RENDERER_ACTION_SUBSCRIBE: (u8, u8) = (17, 1);

    /// Request leg of a call, and the start leg of a subscription.
    pub const MESSAGE_TYPE_REQUEST: u8 = 0;
    /// Response leg of a call, and an item of a subscription.
    pub const MESSAGE_TYPE_RESPONSE: u8 = 1;
    /// Terminal leg of a subscription.
    pub const MESSAGE_TYPE_INTERRUPT: u8 = 2;
    /// The subscriber's cancellation of a subscription.
    pub const MESSAGE_TYPE_STOP: u8 = 3;
}

/// The bot's identity, as the host and the transcript see it.
pub const BOT_ID: &str = "counter-bot";
/// The room the bot posts into.
pub const ROOM_ID: &str = "counter";
/// The `Custom` message discriminator the host hands back in the render context.
pub const MESSAGE_TYPE: &str = "counter/v1";
/// The action a tap on the button triggers.
pub const BUMP_ACTION: &str = "bump";

/// One wire envelope, as `truapi::frame::ProtocolMessage` encodes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Per-message identifier shared by both legs of an exchange.
    pub request_id: String,
    /// Trait discriminant.
    pub trait_id: u8,
    /// Method discriminant within the trait.
    pub method_id: u8,
    /// Which leg this frame carries.
    pub message_type: u8,
    /// The leg's own SCALE-encoded versioned wrapper, inlined.
    pub payload: Vec<u8>,
}

impl Frame {
    /// Encode as `[requestId][trait][method][type][payload...]`.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = self.request_id.encode();
        bytes.push(self.trait_id);
        bytes.push(self.method_id);
        bytes.push(self.message_type);
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    /// Decode one transport frame; the payload runs to the end of the bytes.
    pub fn decode(mut bytes: &[u8]) -> Option<Self> {
        let request_id = String::decode(&mut bytes).ok()?;
        let [trait_id, method_id, message_type, payload @ ..] = bytes else {
            return None;
        };
        Some(Self {
            request_id,
            trait_id: *trait_id,
            method_id: *method_id,
            message_type: *message_type,
            payload: payload.to_vec(),
        })
    }

    fn request(id: String, ids: (u8, u8), payload: impl Encode) -> Self {
        Self {
            request_id: id,
            trait_id: ids.0,
            method_id: ids.1,
            message_type: wire::MESSAGE_TYPE_REQUEST,
            payload: payload.encode(),
        }
    }
}

/// What the guest is waiting on under a request id it minted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Handshake,
    RegisterBot,
    CreateRoom,
    PostMessage,
}

/// What the guest learned, in the order it learned it. The embedder logs
/// these; they are the guest's side of the evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The host agreed on the wire codec version.
    HandshakeOk,
    /// A call the guest made failed.
    CallFailed {
        /// Which call.
        call: &'static str,
        /// The host's answer, as `CallError` displays it.
        error: String,
    },
    /// The bot is registered.
    BotRegistered,
    /// The room exists.
    RoomCreated,
    /// The message was stored under this id.
    MessagePosted {
        /// Host-assigned message id.
        message_id: String,
    },
    /// The host asked the guest to draw a body it owns.
    RenderOpened {
        /// Host-minted subscription id.
        request_id: String,
        /// The message id or the card id from the render context.
        body: String,
    },
    /// The host closed a render stream.
    RenderClosed {
        /// Host-minted subscription id.
        request_id: String,
    },
    /// A tap reached the guest.
    ActionReceived {
        /// Action id from the tree.
        action_id: String,
    },
    /// The guest sent a replacement tree.
    TreeSent {
        /// The count it shows.
        count: u32,
        /// Whether it shows the paused marker.
        paused: bool,
    },
}

/// Which surface the bot draws for, decided at construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// A chat message the bot posts itself.
    #[default]
    Chat,
    /// A Pocket card the host's manifest (or debug settings) declares.
    Pocket,
    /// Both: the chat message it posts and the Pocket card, one count.
    Unified,
}

/// The counter bot. Pure state machine: frames in, frames and events out.
#[derive(Debug, Default)]
pub struct Worker {
    mode: Mode,
    count: u32,
    paused: bool,
    next_id: u64,
    pending: Vec<(String, Pending)>,
    message_id: Option<String>,
    render_streams: Vec<String>,
    events: Vec<Event>,
}

impl Worker {
    /// A fresh chat bot with the count at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// A fresh Pocket-card bot with the count at zero.
    pub fn pocket() -> Self {
        Self {
            mode: Mode::Pocket,
            ..Self::default()
        }
    }

    /// A fresh bot that draws its chat message and a Pocket card from one
    /// count, starting at zero.
    pub fn unified() -> Self {
        Self {
            mode: Mode::Unified,
            ..Self::default()
        }
    }

    /// Events recorded since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<Event> {
        core::mem::take(&mut self.events)
    }

    fn mint(&mut self, pending: Pending) -> String {
        self.next_id += 1;
        let id = format!("w:{}", self.next_id);
        self.pending.push((id.clone(), pending));
        id
    }

    /// Frames to send when the connection opens. Every mode handshakes and
    /// subscribes to renderer actions; chat and unified modes also register
    /// the bot and create the room, and post the message once the room exists.
    pub fn start(&mut self) -> Vec<Frame> {
        let handshake = Frame::request(
            self.mint(Pending::Handshake),
            wire::SYSTEM_HANDSHAKE,
            versioned::system::HostHandshakeRequest::V1(HostHandshakeRequest {
                codec_version: WIRE_CODEC_VERSION,
            }),
        );
        if self.mode == Mode::Pocket {
            return vec![handshake, self.subscribe_actions()];
        }
        let register_bot = Frame::request(
            self.mint(Pending::RegisterBot),
            wire::CHAT_REGISTER_BOT,
            versioned::chat::HostChatRegisterBotRequest::V1(HostChatRegisterBotRequest {
                bot_id: BOT_ID.to_string(),
                name: "Counter".to_string(),
                icon: String::new(),
            }),
        );
        let create_room = Frame::request(
            self.mint(Pending::CreateRoom),
            wire::CHAT_CREATE_ROOM,
            versioned::chat::HostChatCreateRoomRequest::V1(HostChatCreateRoomRequest {
                room_id: ROOM_ID.to_string(),
                // A host lists a unified product's room beside a chat-only counter's.
                name: if self.mode == Mode::Unified {
                    "Shared counter"
                } else {
                    "Counter"
                }
                .to_string(),
                icon: String::new(),
            }),
        );
        let subscribe_actions = self.subscribe_actions();
        vec![handshake, register_bot, create_room, subscribe_actions]
    }

    fn subscribe_actions(&mut self) -> Frame {
        self.next_id += 1;
        Frame {
            request_id: format!("w:{}", self.next_id),
            trait_id: wire::RENDERER_ACTION_SUBSCRIBE.0,
            method_id: wire::RENDERER_ACTION_SUBSCRIBE.1,
            message_type: wire::MESSAGE_TYPE_REQUEST,
            payload: versioned::renderer::HostRendererActionSubscribeRequest::V1.encode(),
        }
    }

    /// Handle one frame from the host and answer with the frames it causes.
    pub fn on_frame(&mut self, bytes: &[u8]) -> Vec<Frame> {
        let Some(frame) = Frame::decode(bytes) else {
            return Vec::new();
        };
        let ids = (frame.trait_id, frame.method_id);
        if ids == wire::RENDERER_RENDER {
            return self.on_render_frame(frame);
        }
        if ids == wire::RENDERER_ACTION_SUBSCRIBE {
            return self.on_action_frame(&frame);
        }
        let position = self
            .pending
            .iter()
            .position(|(id, _)| *id == frame.request_id);
        let Some(position) = position else {
            return Vec::new();
        };
        let (_, pending) = self.pending.remove(position);
        match pending {
            Pending::Handshake => {
                self.settle::<versioned::system::HostHandshakeResponse, versioned::system::HostHandshakeError>(
                    "system.handshake",
                    &frame.payload,
                    |_| Event::HandshakeOk,
                );
                Vec::new()
            }
            Pending::RegisterBot => {
                self.settle::<versioned::chat::HostChatRegisterBotResponse, versioned::chat::HostChatRegisterBotError>(
                    "chat.registerBot",
                    &frame.payload,
                    |_| Event::BotRegistered,
                );
                Vec::new()
            }
            Pending::CreateRoom => {
                let created = self.settle::<versioned::chat::HostChatCreateRoomResponse, versioned::chat::HostChatCreateRoomError>(
                    "chat.createRoom",
                    &frame.payload,
                    |_| Event::RoomCreated,
                );
                if created {
                    vec![self.post_message()]
                } else {
                    Vec::new()
                }
            }
            Pending::PostMessage => {
                self.settle::<versioned::chat::HostChatPostMessageResponse, versioned::chat::HostChatPostMessageError>(
                    "chat.postMessage",
                    &frame.payload,
                    |response| {
                        let versioned::chat::HostChatPostMessageResponse::V1(response) = response;
                        Event::MessagePosted {
                            message_id: response.message_id,
                        }
                    },
                );
                if let Some(Event::MessagePosted { message_id }) = self.events.last() {
                    self.message_id = Some(message_id.clone());
                }
                Vec::new()
            }
        }
    }

    /// The host paused the worker. Trees redraw with the marker.
    pub fn on_suspend(&mut self) -> Vec<Frame> {
        self.paused = true;
        self.redraw()
    }

    /// The host resumed the worker.
    pub fn on_resume(&mut self) -> Vec<Frame> {
        self.paused = false;
        self.redraw()
    }

    /// Decode a response leg and record the outcome. Answers whether it
    /// succeeded.
    fn settle<Response, Error>(
        &mut self,
        call: &'static str,
        payload: &[u8],
        on_ok: impl FnOnce(Response) -> Event,
    ) -> bool
    where
        Response: Decode,
        Error: Decode + core::fmt::Debug,
    {
        let decoded = Result::<Response, CallError<Error>>::decode_all(&mut &payload[..]);
        match decoded {
            Ok(Ok(response)) => {
                self.events.push(on_ok(response));
                true
            }
            Ok(Err(error)) => {
                self.events.push(Event::CallFailed {
                    call,
                    error: format!("{error:?}"),
                });
                false
            }
            Err(error) => {
                self.events.push(Event::CallFailed {
                    call,
                    error: format!("undecodable response: {error}"),
                });
                false
            }
        }
    }

    fn post_message(&mut self) -> Frame {
        Frame::request(
            self.mint(Pending::PostMessage),
            wire::CHAT_POST_MESSAGE,
            versioned::chat::HostChatPostMessageRequest::V1(HostChatPostMessageRequest {
                room_id: ROOM_ID.to_string(),
                payload: ChatMessageContent::Custom(ChatCustomMessage {
                    message_type: MESSAGE_TYPE.to_string(),
                    payload: Vec::new(),
                }),
            }),
        )
    }

    fn on_render_frame(&mut self, frame: Frame) -> Vec<Frame> {
        match frame.message_type {
            wire::MESSAGE_TYPE_REQUEST => {
                let request = versioned::renderer::ProductRendererRenderRequest::decode_all(
                    &mut &frame.payload[..],
                );
                let Ok(versioned::renderer::ProductRendererRenderRequest::V1(request)) = request
                else {
                    return Vec::new();
                };
                let body = match (self.mode, request.context) {
                    (Mode::Chat | Mode::Unified, RenderContext::ChatMessage { message_id, .. }) => {
                        message_id
                    }
                    (Mode::Pocket | Mode::Unified, RenderContext::PocketCard { card_id }) => {
                        card_id
                    }
                    _ => return Vec::new(),
                };
                self.events.push(Event::RenderOpened {
                    request_id: frame.request_id.clone(),
                    body,
                });
                self.render_streams.push(frame.request_id.clone());
                vec![self.tree_frame(frame.request_id)]
            }
            wire::MESSAGE_TYPE_STOP => {
                self.render_streams.retain(|id| *id != frame.request_id);
                self.events.push(Event::RenderClosed {
                    request_id: frame.request_id,
                });
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn on_action_frame(&mut self, frame: &Frame) -> Vec<Frame> {
        if frame.message_type != wire::MESSAGE_TYPE_RESPONSE {
            return Vec::new();
        }
        let item = versioned::renderer::HostRendererActionSubscribeItem::decode_all(
            &mut &frame.payload[..],
        );
        let Ok(versioned::renderer::HostRendererActionSubscribeItem::V1(item)) = item else {
            return Vec::new();
        };
        self.events.push(Event::ActionReceived {
            action_id: item.action_id.clone(),
        });
        if item.action_id != BUMP_ACTION {
            return Vec::new();
        }
        self.count += 1;
        self.redraw()
    }

    fn redraw(&mut self) -> Vec<Frame> {
        let streams = self.render_streams.clone();
        streams
            .into_iter()
            .map(|request_id| self.tree_frame(request_id))
            .collect()
    }

    fn tree_frame(&mut self, request_id: String) -> Frame {
        self.events.push(Event::TreeSent {
            count: self.count,
            paused: self.paused,
        });
        Frame {
            request_id,
            trait_id: wire::RENDERER_RENDER.0,
            method_id: wire::RENDERER_RENDER.1,
            message_type: wire::MESSAGE_TYPE_RESPONSE,
            payload: versioned::renderer::ProductRendererRenderItem::V1(self.tree()).encode(),
        }
    }

    /// The body: the count, the paused marker when set, and one button.
    pub fn tree(&self) -> RendererNode {
        let label = if self.paused {
            format!("count {} · paused", self.count)
        } else {
            format!("count {}", self.count)
        };
        RendererNode::Column {
            modifiers: Vec::new(),
            props: ColumnProps {
                horizontal_alignment: None,
                vertical_arrangement: None,
            },
            children: vec![
                RendererNode::Text {
                    modifiers: Vec::new(),
                    props: TextProps {
                        style: None,
                        color: None,
                    },
                    children: vec![RendererNode::String { text: label }],
                },
                RendererNode::Button {
                    modifiers: Vec::new(),
                    props: ButtonProps {
                        text: BUMP_ACTION.to_string(),
                        variant: None,
                        enabled: parity_scale_codec::OptionBool(None),
                        loading: parity_scale_codec::OptionBool(None),
                        click_action: Some(BUMP_ACTION.to_string()),
                    },
                    children: Vec::new(),
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use truapi::v01::{
        ChatBotRegistrationStatus, ChatRoomRegistrationStatus, HostChatCreateRoomResponse,
        HostChatPostMessageResponse, HostChatRegisterBotResponse, HostRendererActionSubscribeItem,
        ProductRendererRenderRequest,
    };

    fn response<Response: Encode, Error: Encode>(
        request_id: &str,
        ids: (u8, u8),
        result: Result<Response, CallError<Error>>,
    ) -> Vec<u8> {
        Frame {
            request_id: request_id.to_string(),
            trait_id: ids.0,
            method_id: ids.1,
            message_type: wire::MESSAGE_TYPE_RESPONSE,
            payload: result.encode(),
        }
        .encode()
    }

    fn decoded(frame: &Frame) -> Frame {
        Frame::decode(&frame.encode()).expect("a frame the guest sent decodes")
    }

    /// Drive the bot to the point where its message is stored.
    fn posted_bot() -> Worker {
        posted(Worker::new())
    }

    fn posted(mut worker: Worker) -> Worker {
        let start = worker.start();
        assert_eq!(
            start
                .iter()
                .map(|frame| (frame.trait_id, frame.method_id))
                .collect::<Vec<_>>(),
            vec![
                wire::SYSTEM_HANDSHAKE,
                wire::CHAT_REGISTER_BOT,
                wire::CHAT_CREATE_ROOM,
                wire::RENDERER_ACTION_SUBSCRIBE
            ]
        );
        assert_eq!(decoded(&start[0]), start[0]);
        worker.on_frame(&response::<_, versioned::system::HostHandshakeError>(
            &start[0].request_id,
            wire::SYSTEM_HANDSHAKE,
            Ok(versioned::system::HostHandshakeResponse::V1),
        ));
        worker.on_frame(&response::<_, versioned::chat::HostChatRegisterBotError>(
            &start[1].request_id,
            wire::CHAT_REGISTER_BOT,
            Ok(versioned::chat::HostChatRegisterBotResponse::V1(
                HostChatRegisterBotResponse {
                    status: ChatBotRegistrationStatus::New,
                },
            )),
        ));
        let after_room = worker.on_frame(&response::<_, versioned::chat::HostChatCreateRoomError>(
            &start[2].request_id,
            wire::CHAT_CREATE_ROOM,
            Ok(versioned::chat::HostChatCreateRoomResponse::V1(
                HostChatCreateRoomResponse {
                    status: ChatRoomRegistrationStatus::New,
                },
            )),
        ));
        assert_eq!(
            after_room.len(),
            1,
            "the message is posted once the room exists"
        );
        let post = &after_room[0];
        assert_eq!((post.trait_id, post.method_id), wire::CHAT_POST_MESSAGE);
        worker.on_frame(&response::<_, versioned::chat::HostChatPostMessageError>(
            &post.request_id,
            wire::CHAT_POST_MESSAGE,
            Ok(versioned::chat::HostChatPostMessageResponse::V1(
                HostChatPostMessageResponse {
                    message_id: "m1".to_string(),
                },
            )),
        ));
        assert_eq!(
            worker.take_events(),
            vec![
                Event::HandshakeOk,
                Event::BotRegistered,
                Event::RoomCreated,
                Event::MessagePosted {
                    message_id: "m1".to_string()
                }
            ]
        );
        worker
    }

    fn render_start(request_id: &str) -> Vec<u8> {
        Frame {
            request_id: request_id.to_string(),
            trait_id: wire::RENDERER_RENDER.0,
            method_id: wire::RENDERER_RENDER.1,
            message_type: wire::MESSAGE_TYPE_REQUEST,
            payload: versioned::renderer::ProductRendererRenderRequest::V1(
                ProductRendererRenderRequest {
                    context: RenderContext::ChatMessage {
                        room_id: ROOM_ID.to_string(),
                        message_id: "m1".to_string(),
                        message_type: MESSAGE_TYPE.to_string(),
                    },
                    payload: Vec::new(),
                },
            )
            .encode(),
        }
        .encode()
    }

    fn tree_count(frame: &Frame) -> String {
        let versioned::renderer::ProductRendererRenderItem::V1(node) =
            versioned::renderer::ProductRendererRenderItem::decode_all(&mut &frame.payload[..])
                .expect("a tree the guest sent decodes");
        let RendererNode::Column { children, .. } = node else {
            panic!("the body is a column")
        };
        let RendererNode::Text { children, .. } = &children[0] else {
            panic!("the first child is the label")
        };
        let RendererNode::String { text } = &children[0] else {
            panic!("the label is a string")
        };
        text.clone()
    }

    #[test]
    fn a_render_request_is_answered_with_the_count_and_a_tap_bumps_it() {
        let mut worker = posted_bot();
        let trees = worker.on_frame(&render_start("h:1"));
        assert_eq!(trees.len(), 1);
        assert_eq!(trees[0].request_id, "h:1");
        assert_eq!(trees[0].message_type, wire::MESSAGE_TYPE_RESPONSE);
        assert_eq!(tree_count(&trees[0]), "count 0");

        let tap = Frame {
            request_id: "w:4".to_string(),
            trait_id: wire::RENDERER_ACTION_SUBSCRIBE.0,
            method_id: wire::RENDERER_ACTION_SUBSCRIBE.1,
            message_type: wire::MESSAGE_TYPE_RESPONSE,
            payload: versioned::renderer::HostRendererActionSubscribeItem::V1(
                HostRendererActionSubscribeItem {
                    context: RenderContext::ChatMessage {
                        room_id: ROOM_ID.to_string(),
                        message_id: "m1".to_string(),
                        message_type: MESSAGE_TYPE.to_string(),
                    },
                    action_id: BUMP_ACTION.to_string(),
                    payload: Vec::new(),
                },
            )
            .encode(),
        }
        .encode();
        let trees = worker.on_frame(&tap);
        assert_eq!(trees.len(), 1, "every open render stream is redrawn");
        assert_eq!(tree_count(&trees[0]), "count 1");
        assert_eq!(
            worker.take_events(),
            vec![
                Event::RenderOpened {
                    request_id: "h:1".to_string(),
                    body: "m1".to_string()
                },
                Event::TreeSent {
                    count: 0,
                    paused: false
                },
                Event::ActionReceived {
                    action_id: BUMP_ACTION.to_string()
                },
                Event::TreeSent {
                    count: 1,
                    paused: false
                },
            ]
        );
    }

    #[test]
    fn suspend_and_resume_redraw_open_streams_only() {
        let mut worker = posted_bot();
        assert!(worker.on_suspend().is_empty(), "nothing is open yet");
        worker.on_frame(&render_start("h:2"));
        let paused = worker.on_resume();
        assert_eq!(tree_count(&paused[0]), "count 0");
        let marked = worker.on_suspend();
        assert_eq!(tree_count(&marked[0]), "count 0 · paused");
        let stop = Frame {
            request_id: "h:2".to_string(),
            trait_id: wire::RENDERER_RENDER.0,
            method_id: wire::RENDERER_RENDER.1,
            message_type: wire::MESSAGE_TYPE_STOP,
            payload: Vec::new(),
        }
        .encode();
        worker.on_frame(&stop);
        assert!(
            worker.on_resume().is_empty(),
            "a stopped stream is not redrawn"
        );
    }

    fn pocket_render_start(request_id: &str) -> Vec<u8> {
        Frame {
            request_id: request_id.to_string(),
            trait_id: wire::RENDERER_RENDER.0,
            method_id: wire::RENDERER_RENDER.1,
            message_type: wire::MESSAGE_TYPE_REQUEST,
            payload: versioned::renderer::ProductRendererRenderRequest::V1(
                ProductRendererRenderRequest {
                    context: RenderContext::PocketCard {
                        card_id: "counter".to_string(),
                    },
                    payload: Vec::new(),
                },
            )
            .encode(),
        }
        .encode()
    }

    #[test]
    fn pocket_mode_makes_no_chat_call_and_draws_the_card() {
        let mut worker = Worker::pocket();
        let start = worker.start();
        assert_eq!(
            start
                .iter()
                .map(|frame| (frame.trait_id, frame.method_id))
                .collect::<Vec<_>>(),
            vec![wire::SYSTEM_HANDSHAKE, wire::RENDERER_ACTION_SUBSCRIBE]
        );
        let trees = worker.on_frame(&pocket_render_start("h:7"));
        assert_eq!(tree_count(&trees[0]), "count 0");
        assert!(
            worker.on_frame(&render_start("h:8")).is_empty(),
            "a chat message context is not this mode's body"
        );
        assert_eq!(
            worker.take_events(),
            vec![
                Event::RenderOpened {
                    request_id: "h:7".to_string(),
                    body: "counter".to_string()
                },
                Event::TreeSent {
                    count: 0,
                    paused: false
                },
            ]
        );
    }

    fn tap(context: RenderContext) -> Vec<u8> {
        Frame {
            request_id: "w:4".to_string(),
            trait_id: wire::RENDERER_ACTION_SUBSCRIBE.0,
            method_id: wire::RENDERER_ACTION_SUBSCRIBE.1,
            message_type: wire::MESSAGE_TYPE_RESPONSE,
            payload: versioned::renderer::HostRendererActionSubscribeItem::V1(
                HostRendererActionSubscribeItem {
                    context,
                    action_id: BUMP_ACTION.to_string(),
                    payload: Vec::new(),
                },
            )
            .encode(),
        }
        .encode()
    }

    fn counts(trees: &[Frame]) -> Vec<(String, String)> {
        trees
            .iter()
            .map(|tree| (tree.request_id.clone(), tree_count(tree)))
            .collect()
    }

    // One product, one worker: the chat message and the Pocket card are two
    // views of the same count, so a press in either moves both.
    #[test]
    fn unified_mode_draws_its_message_and_its_card_from_one_count() {
        let mut worker = posted(Worker::unified());
        assert_eq!(
            counts(&worker.on_frame(&render_start("h:1"))),
            [("h:1".into(), "count 0".into())]
        );
        assert_eq!(
            counts(&worker.on_frame(&pocket_render_start("h:2"))),
            [("h:2".into(), "count 0".into())]
        );

        let from_card = worker.on_frame(&tap(RenderContext::PocketCard {
            card_id: "counter".to_string(),
        }));
        assert_eq!(
            counts(&from_card),
            [
                ("h:1".into(), "count 1".into()),
                ("h:2".into(), "count 1".into())
            ]
        );

        let from_message = worker.on_frame(&tap(RenderContext::ChatMessage {
            room_id: ROOM_ID.to_string(),
            message_id: "m1".to_string(),
            message_type: MESSAGE_TYPE.to_string(),
        }));
        assert_eq!(
            counts(&from_message),
            [
                ("h:1".into(), "count 2".into()),
                ("h:2".into(), "count 2".into())
            ]
        );
    }

    #[test]
    fn a_refused_room_posts_nothing() {
        let mut worker = Worker::new();
        let start = worker.start();
        let refused = worker.on_frame(&response::<versioned::chat::HostChatCreateRoomResponse, _>(
            &start[2].request_id,
            wire::CHAT_CREATE_ROOM,
            Err(CallError::<versioned::chat::HostChatCreateRoomError>::Denied),
        ));
        assert!(refused.is_empty());
        assert_eq!(
            worker.take_events(),
            vec![Event::CallFailed {
                call: "chat.createRoom",
                error: "Denied".to_string()
            }]
        );
    }
}
