//! A counter bot that runs as a Worker product without JavaScript, written on
//! [`truapi_worker`]: the library owns the sandbox ABI and the wire, this
//! crate owns the count and how it looks.
//!
//! Three modes share the counter, the tree and the lifecycle:
//!
//! - [`Mode::Chat`] (the default, what the CLI proof runs): on start it
//!   registers a bot, creates a room and, once the room exists, posts one
//!   `Custom` message; it draws that message.
//! - [`Mode::Pocket`] (the `pocket` cargo feature): it makes no Chat call,
//!   because a host that serves Pocket workers need not serve Chat, and a
//!   refused call would be noise rather than a finding. It draws a Pocket card.
//! - [`Mode::Unified`] (the `unified` cargo feature): starts as chat mode does,
//!   and draws both its chat message and a Pocket card, so one product shows
//!   the same count on both surfaces.
//!
//! Each `bump` action, from whichever body it was pressed in, increments the
//! one count and redraws every open body.

use truapi_worker::parity_scale_codec::OptionBool;
use truapi_worker::truapi::latest::{
    ButtonProps, ChatCustomMessage, ChatMessageContent, ColumnProps, HostChatCreateRoomRequest,
    HostChatPostMessageRequest, HostChatRegisterBotRequest, HostRendererActionSubscribeItem,
    RenderContext, RendererNode, TextProps,
};
use truapi_worker::{ChatCreateRoom, ChatPostMessage, ChatRegisterBot, Ctx, ProductWorker};

/// The bot's identity, as the host and the transcript see it.
pub const BOT_ID: &str = "counter-bot";
/// The room the bot posts into.
pub const ROOM_ID: &str = "counter";
/// The `Custom` message discriminator the host hands back in the render context.
pub const MESSAGE_TYPE: &str = "counter/v1";
/// The action a tap on the button triggers.
pub const BUMP_ACTION: &str = "bump";

/// Which surfaces the bot draws for, decided at construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A chat message the bot posts itself.
    Chat,
    /// A Pocket card the host's manifest (or debug settings) declares.
    Pocket,
    /// Both: the chat message it posts and the Pocket card, one count.
    Unified,
}

/// What the counter logs, beside the library's own lines.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "the derived Debug output is the log line, and reads every field"
)]
enum Event {
    BotRegistered,
    RoomCreated,
    MessagePosted { message_id: String },
    TreeSent { count: u32, paused: bool },
}

/// The counter bot.
#[derive(Debug)]
pub struct Counter {
    mode: Mode,
    count: u32,
}

impl Counter {
    /// A fresh bot drawing for `mode`, with the count at zero.
    pub fn new(mode: Mode) -> Self {
        Self { mode, count: 0 }
    }

    /// The count it shows.
    pub fn count(&self) -> u32 {
        self.count
    }

    fn post_message(cx: &mut Ctx<Self>) {
        cx.call::<ChatPostMessage>(
            HostChatPostMessageRequest {
                room_id: ROOM_ID.to_string(),
                payload: ChatMessageContent::Custom(ChatCustomMessage {
                    message_type: MESSAGE_TYPE.to_string(),
                    payload: Vec::new(),
                }),
            },
            |_, reply, cx| {
                if let Ok(response) = reply {
                    log(
                        cx,
                        Event::MessagePosted {
                            message_id: response.message_id,
                        },
                    );
                }
            },
        );
    }
}

impl ProductWorker for Counter {
    fn start(&mut self, cx: &mut Ctx<Self>) {
        if self.mode == Mode::Pocket {
            return;
        }
        cx.call::<ChatRegisterBot>(
            HostChatRegisterBotRequest {
                bot_id: BOT_ID.to_string(),
                name: "Counter".to_string(),
                icon: String::new(),
            },
            |_, reply, cx| {
                if reply.is_ok() {
                    log(cx, Event::BotRegistered);
                }
            },
        );
        cx.call::<ChatCreateRoom>(
            HostChatCreateRoomRequest {
                room_id: ROOM_ID.to_string(),
                // A host lists a unified product's room beside a chat-only counter's.
                name: if self.mode == Mode::Unified {
                    "Shared counter"
                } else {
                    "Counter"
                }
                .to_string(),
                icon: String::new(),
            },
            |_, reply, cx| {
                if reply.is_ok() {
                    log(cx, Event::RoomCreated);
                    Self::post_message(cx);
                }
            },
        );
    }

    fn draws(&self, body: &RenderContext) -> bool {
        matches!(
            (self.mode, body),
            (
                Mode::Chat | Mode::Unified,
                RenderContext::ChatMessage { .. }
            ) | (
                Mode::Pocket | Mode::Unified,
                RenderContext::PocketCard { .. }
            )
        )
    }

    fn render(&self, _body: &RenderContext, cx: &mut Ctx<Self>) -> RendererNode {
        let paused = cx.paused();
        log(
            cx,
            Event::TreeSent {
                count: self.count,
                paused,
            },
        );
        tree(self.count, paused)
    }

    fn action(&mut self, action: &HostRendererActionSubscribeItem, cx: &mut Ctx<Self>) {
        if action.action_id == BUMP_ACTION {
            self.count += 1;
            cx.redraw();
        }
    }
}

fn log(cx: &mut Ctx<Counter>, event: Event) {
    cx.log(format_args!("{event:?}"));
}

/// The body: the count, the paused marker when set, and one button.
fn tree(count: u32, paused: bool) -> RendererNode {
    let label = if paused {
        format!("count {count} · paused")
    } else {
        format!("count {count}")
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
                    enabled: OptionBool(None),
                    loading: OptionBool(None),
                    click_action: Some(BUMP_ACTION.to_string()),
                },
                children: Vec::new(),
            },
        ],
    }
}

#[cfg(all(feature = "pocket", feature = "unified"))]
compile_error!("`pocket` and `unified` select different modes; enable one");

/// The mode the wasm32 build exports, chosen by cargo feature.
pub const BUILD_MODE: Mode = if cfg!(feature = "unified") {
    Mode::Unified
} else if cfg!(feature = "pocket") {
    Mode::Pocket
} else {
    Mode::Chat
};

truapi_worker::export_worker!(Counter::new(BUILD_MODE));

#[cfg(test)]
mod tests {
    use truapi_worker::testing::{Turn, action, render, reply, tree};
    use truapi_worker::{Instance, SystemHandshake};

    use super::*;

    fn label(frame: &[u8]) -> String {
        let Some(RendererNode::Column { children, .. }) = tree(frame) else {
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

    fn labels(instance: &mut Instance<Counter>, turn: Turn) -> Vec<String> {
        instance
            .run(&turn)
            .sent
            .iter()
            .map(|frame| label(frame))
            .collect()
    }

    fn message() -> RenderContext {
        RenderContext::ChatMessage {
            room_id: ROOM_ID.to_string(),
            message_id: "m1".to_string(),
            message_type: MESSAGE_TYPE.to_string(),
        }
    }

    fn card() -> RenderContext {
        RenderContext::PocketCard {
            card_id: "counter".to_string(),
        }
    }

    // One product, one worker: the chat message and the Pocket card are two
    // views of the same count, so a press in either moves both.
    #[test]
    fn unified_mode_draws_its_message_and_its_card_from_one_count() {
        let mut instance = Instance::new(Counter::new(Mode::Unified));
        instance.run(&Turn::Start);
        assert_eq!(
            labels(&mut instance, Turn::Frame(render("h:1", message()))),
            ["count 0"]
        );
        assert_eq!(
            labels(&mut instance, Turn::Frame(render("h:2", card()))),
            ["count 0"]
        );

        let from_card = Turn::Frame(action("w:4", card(), BUMP_ACTION));
        assert_eq!(labels(&mut instance, from_card), ["count 1", "count 1"]);
        let from_message = Turn::Frame(action("w:4", message(), BUMP_ACTION));
        assert_eq!(labels(&mut instance, from_message), ["count 2", "count 2"]);
    }

    // A host that serves Pocket workers need not serve Chat.
    #[test]
    fn pocket_mode_makes_no_chat_call_and_draws_only_the_card() {
        let mut instance = Instance::new(Counter::new(Mode::Pocket));
        let opened = instance.run(&Turn::Start).sent.len();
        assert_eq!(opened, 2, "the handshake and the action subscription only");
        instance.run(&Turn::Frame(reply::<SystemHandshake>("w:1", Ok(()))));
        assert!(
            instance
                .run(&Turn::Frame(render("h:8", message())))
                .sent
                .is_empty()
        );
        assert_eq!(
            labels(&mut instance, Turn::Frame(render("h:7", card()))),
            ["count 0"]
        );
    }
}
