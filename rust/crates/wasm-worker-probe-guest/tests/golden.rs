//! Golden transcripts: every outbound frame and log line the counter guest
//! produces for a scripted host, in each build mode, compared byte for byte.
//!
//! A transcript is a text file of turns. `start`, `frame <hex>`, `suspend` and
//! `resume` are what the host does; the `send <hex>` and `log <text>` lines
//! after one are what the guest answered in that turn. The files pin the
//! guest's wire behaviour, so a refactor that changes a byte fails here.
//! `UPDATE_GOLDEN=1 cargo test -p wasm-worker-probe-guest --test golden`
//! rewrites them from the current guest.

use std::fmt::Write as _;
use std::path::PathBuf;

use parity_scale_codec::Encode;
use truapi::v01::{
    ChatBotRegistrationStatus, ChatRoomRegistrationStatus, HostChatCreateRoomResponse,
    HostChatPostMessageResponse, HostChatRegisterBotResponse, HostRendererActionSubscribeItem,
    ProductRendererRenderRequest, RenderContext,
};
use truapi::{CallError, versioned};
use wasm_worker_probe_guest::{BUMP_ACTION, Frame, MESSAGE_TYPE, ROOM_ID, Worker, wire};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Input {
    Start,
    Frame(Vec<u8>),
    Suspend,
    Resume,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Answer {
    sent: Vec<Vec<u8>>,
    logs: Vec<String>,
}

fn run(worker: &mut Worker, input: &Input) -> Answer {
    let frames = match input {
        Input::Start => worker.start(),
        Input::Frame(bytes) => worker.on_frame(bytes),
        Input::Suspend => worker.on_suspend(),
        Input::Resume => worker.on_resume(),
    };
    Answer {
        sent: frames.iter().map(Frame::encode).collect(),
        logs: worker
            .take_events()
            .iter()
            .map(|event| format!("{event:?}"))
            .collect(),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn from_hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("hex byte"))
        .collect()
}

fn render(script: &[(Input, Answer)]) -> String {
    let mut out = String::new();
    for (input, answer) in script {
        match input {
            Input::Start => out.push_str("start\n"),
            Input::Frame(bytes) => writeln!(out, "frame {}", to_hex(bytes)).expect("write"),
            Input::Suspend => out.push_str("suspend\n"),
            Input::Resume => out.push_str("resume\n"),
        }
        for frame in &answer.sent {
            writeln!(out, "  send {}", to_hex(frame)).expect("write");
        }
        for line in &answer.logs {
            writeln!(out, "  log {line}").expect("write");
        }
    }
    out
}

fn parse(text: &str) -> Vec<(Input, Answer)> {
    let mut script: Vec<(Input, Answer)> = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let input = match word {
            "start" => Input::Start,
            "frame" => Input::Frame(from_hex(rest)),
            "suspend" => Input::Suspend,
            "resume" => Input::Resume,
            "send" => {
                let (_, answer) = script.last_mut().expect("a send follows a turn");
                answer.sent.push(from_hex(rest));
                continue;
            }
            "log" => {
                let (_, answer) = script.last_mut().expect("a log follows a turn");
                answer.logs.push(rest.to_string());
                continue;
            }
            other => panic!("unknown transcript line `{other}`"),
        };
        script.push((input, Answer::default()));
    }
    script
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"))
}

/// Replay `inputs` against `worker` and compare with the stored transcript,
/// or rewrite it when `UPDATE_GOLDEN` is set.
fn check(name: &str, mut worker: Worker, inputs: Vec<Input>) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let script: Vec<_> = inputs
            .into_iter()
            .map(|input| {
                let answer = run(&mut worker, &input);
                (input, answer)
            })
            .collect();
        std::fs::write(&path, render(&script)).expect("write golden transcript");
        return;
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let script = parse(&text);
    assert_eq!(
        script
            .iter()
            .map(|(input, _)| input.clone())
            .collect::<Vec<_>>(),
        inputs,
        "{name}: the stored host script drifted from this test's"
    );
    for (turn, (input, expected)) in script.iter().enumerate() {
        assert_eq!(
            &run(&mut worker, input),
            expected,
            "{name}: turn {turn} ({input:?})"
        );
    }
}

fn reply<Response: Encode, Error: Encode>(
    request_id: &str,
    ids: (u8, u8),
    result: Result<Response, CallError<Error>>,
) -> Input {
    Input::Frame(
        Frame {
            request_id: request_id.to_string(),
            trait_id: ids.0,
            method_id: ids.1,
            message_type: wire::MESSAGE_TYPE_RESPONSE,
            payload: result.encode(),
        }
        .encode(),
    )
}

fn handshake_ok() -> Input {
    reply::<_, versioned::system::HostHandshakeError>(
        "w:1",
        wire::SYSTEM_HANDSHAKE,
        Ok(versioned::system::HostHandshakeResponse::V1),
    )
}

fn bot_registered() -> Input {
    reply::<_, versioned::chat::HostChatRegisterBotError>(
        "w:2",
        wire::CHAT_REGISTER_BOT,
        Ok(versioned::chat::HostChatRegisterBotResponse::V1(
            HostChatRegisterBotResponse {
                status: ChatBotRegistrationStatus::New,
            },
        )),
    )
}

fn bot_refused() -> Input {
    reply::<versioned::chat::HostChatRegisterBotResponse, _>(
        "w:2",
        wire::CHAT_REGISTER_BOT,
        Err(
            CallError::<versioned::chat::HostChatRegisterBotError>::HostFailure {
                reason: "bot registration is not supported by this host".to_string(),
            },
        ),
    )
}

fn room_created() -> Input {
    reply::<_, versioned::chat::HostChatCreateRoomError>(
        "w:3",
        wire::CHAT_CREATE_ROOM,
        Ok(versioned::chat::HostChatCreateRoomResponse::V1(
            HostChatCreateRoomResponse {
                status: ChatRoomRegistrationStatus::New,
            },
        )),
    )
}

fn room_refused() -> Input {
    reply::<versioned::chat::HostChatCreateRoomResponse, _>(
        "w:3",
        wire::CHAT_CREATE_ROOM,
        Err(CallError::<versioned::chat::HostChatCreateRoomError>::Denied),
    )
}

fn message_posted() -> Input {
    reply::<_, versioned::chat::HostChatPostMessageError>(
        "w:5",
        wire::CHAT_POST_MESSAGE,
        Ok(versioned::chat::HostChatPostMessageResponse::V1(
            HostChatPostMessageResponse {
                message_id: "m1".to_string(),
            },
        )),
    )
}

fn undecodable_post_reply() -> Input {
    Input::Frame(
        Frame {
            request_id: "w:5".to_string(),
            trait_id: wire::CHAT_POST_MESSAGE.0,
            method_id: wire::CHAT_POST_MESSAGE.1,
            message_type: wire::MESSAGE_TYPE_RESPONSE,
            payload: vec![0x07],
        }
        .encode(),
    )
}

fn message_context() -> RenderContext {
    RenderContext::ChatMessage {
        room_id: ROOM_ID.to_string(),
        message_id: "m1".to_string(),
        message_type: MESSAGE_TYPE.to_string(),
    }
}

fn card_context() -> RenderContext {
    RenderContext::PocketCard {
        card_id: "counter".to_string(),
    }
}

fn render_frame(request_id: &str, message_type: u8, context: RenderContext) -> Input {
    Input::Frame(
        Frame {
            request_id: request_id.to_string(),
            trait_id: wire::RENDERER_RENDER.0,
            method_id: wire::RENDERER_RENDER.1,
            message_type,
            payload: if message_type == wire::MESSAGE_TYPE_REQUEST {
                versioned::renderer::ProductRendererRenderRequest::V1(
                    ProductRendererRenderRequest {
                        context,
                        payload: Vec::new(),
                    },
                )
                .encode()
            } else {
                Vec::new()
            },
        }
        .encode(),
    )
}

fn open(request_id: &str, context: RenderContext) -> Input {
    render_frame(request_id, wire::MESSAGE_TYPE_REQUEST, context)
}

fn stop(request_id: &str) -> Input {
    render_frame(request_id, wire::MESSAGE_TYPE_STOP, card_context())
}

fn action(subscription: &str, context: RenderContext, action_id: &str) -> Input {
    Input::Frame(
        Frame {
            request_id: subscription.to_string(),
            trait_id: wire::RENDERER_ACTION_SUBSCRIBE.0,
            method_id: wire::RENDERER_ACTION_SUBSCRIBE.1,
            message_type: wire::MESSAGE_TYPE_RESPONSE,
            payload: versioned::renderer::HostRendererActionSubscribeItem::V1(
                HostRendererActionSubscribeItem {
                    context,
                    action_id: action_id.to_string(),
                    payload: Vec::new(),
                },
            )
            .encode(),
        }
        .encode(),
    )
}

fn stray_reply() -> Input {
    reply::<_, versioned::system::HostHandshakeError>(
        "w:99",
        wire::SYSTEM_HANDSHAKE,
        Ok(versioned::system::HostHandshakeResponse::V1),
    )
}

#[test]
fn chat_mode() {
    check(
        "chat",
        Worker::new(),
        vec![
            Input::Start,
            handshake_ok(),
            bot_refused(),
            room_created(),
            message_posted(),
            open("h:1", message_context()),
            open("h:2", card_context()),
            action("w:4", message_context(), BUMP_ACTION),
            action("w:4", message_context(), "other"),
            Input::Suspend,
            Input::Resume,
            stray_reply(),
            Input::Frame(vec![0xff]),
            stop("h:1"),
            Input::Resume,
        ],
    );
}

#[test]
fn chat_mode_with_a_refused_room() {
    check(
        "chat-room-refused",
        Worker::new(),
        vec![
            Input::Start,
            handshake_ok(),
            bot_registered(),
            room_refused(),
        ],
    );
}

#[test]
fn chat_mode_with_an_undecodable_reply() {
    check(
        "chat-undecodable-reply",
        Worker::new(),
        vec![
            Input::Start,
            handshake_ok(),
            bot_registered(),
            room_created(),
            undecodable_post_reply(),
        ],
    );
}

#[test]
fn pocket_mode() {
    check(
        "pocket",
        Worker::pocket(),
        vec![
            Input::Start,
            handshake_ok(),
            open("h:7", card_context()),
            open("h:8", message_context()),
            action("w:2", card_context(), BUMP_ACTION),
            Input::Suspend,
            Input::Resume,
            stop("h:7"),
            action("w:2", card_context(), BUMP_ACTION),
        ],
    );
}

#[test]
fn unified_mode() {
    check(
        "unified",
        Worker::unified(),
        vec![
            Input::Start,
            handshake_ok(),
            bot_registered(),
            room_created(),
            message_posted(),
            open("h:1", message_context()),
            open("h:2", card_context()),
            action("w:4", card_context(), BUMP_ACTION),
            action("w:4", message_context(), BUMP_ACTION),
            Input::Suspend,
            Input::Resume,
            stop("h:2"),
            action("w:4", message_context(), BUMP_ACTION),
        ],
    );
}
