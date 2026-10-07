//! Golden transcripts: every outbound frame and log line the counter produces
//! for a scripted host, in each build mode, compared byte for byte.
//!
//! The files under `tests/golden` pin the counter's wire behaviour, so a
//! refactor that changes a byte fails here. `wasm-worker-probe` replays the
//! same files against the wasm32 builds in its sandbox, which checks the ABI
//! as well. `UPDATE_GOLDEN=1 cargo test -p wasm-worker-probe-guest --test
//! golden` rewrites them from the current counter.

use std::path::PathBuf;

use truapi_worker::testing::{Transcript, Turn, action, render, reply, stop_render};
use truapi_worker::truapi::CallError;
use truapi_worker::truapi::latest::{
    ChatBotRegistrationStatus, ChatRoomRegistrationStatus, HostChatCreateRoomResponse,
    HostChatPostMessageResponse, HostChatRegisterBotResponse, RenderContext,
};
use truapi_worker::{
    ChatCreateRoom, ChatPostMessage, ChatRegisterBot, Frame, Instance, SystemHandshake, wire,
};
use wasm_worker_probe_guest::{BUMP_ACTION, Counter, MESSAGE_TYPE, Mode, ROOM_ID};

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"))
}

/// Replay `inputs` against a fresh counter and compare with the stored
/// transcript, or rewrite it when `UPDATE_GOLDEN` is set.
fn check(name: &str, mode: Mode, inputs: Vec<Turn>) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let transcript = Transcript::record(Counter::new(mode), inputs);
        std::fs::write(&path, transcript.to_string()).expect("write golden transcript");
        return;
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let stored = Transcript::parse(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!(
        stored.inputs(),
        inputs,
        "{name}: the stored host script drifted from this test's"
    );
    let mut instance = Instance::new(Counter::new(mode));
    for (index, (turn, expected)) in stored.turns.iter().enumerate() {
        assert_eq!(
            &instance.run(turn),
            expected,
            "{name}: turn {index} ({turn:?})"
        );
    }
}

fn host(bytes: Vec<u8>) -> Turn {
    Turn::Frame(bytes)
}

fn handshake_ok() -> Turn {
    host(reply::<SystemHandshake>("w:1", Ok(())))
}

fn bot_registered() -> Turn {
    host(reply::<ChatRegisterBot>(
        "w:2",
        Ok(HostChatRegisterBotResponse {
            status: ChatBotRegistrationStatus::New,
        }),
    ))
}

fn bot_refused() -> Turn {
    host(reply::<ChatRegisterBot>(
        "w:2",
        Err(CallError::HostFailure {
            reason: "bot registration is not supported by this host".to_string(),
        }),
    ))
}

fn room_created() -> Turn {
    host(reply::<ChatCreateRoom>(
        "w:3",
        Ok(HostChatCreateRoomResponse {
            status: ChatRoomRegistrationStatus::New,
        }),
    ))
}

fn room_refused() -> Turn {
    host(reply::<ChatCreateRoom>("w:3", Err(CallError::Denied)))
}

fn message_posted() -> Turn {
    host(reply::<ChatPostMessage>(
        "w:5",
        Ok(HostChatPostMessageResponse {
            message_id: "m1".to_string(),
        }),
    ))
}

fn undecodable_post_reply() -> Turn {
    host(
        Frame::new(
            "w:5".to_string(),
            wire::CHAT_POST_MESSAGE,
            wire::MESSAGE_TYPE_RESPONSE,
            vec![0x07],
        )
        .encode(),
    )
}

fn stray_reply() -> Turn {
    host(reply::<SystemHandshake>("w:99", Ok(())))
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

fn open(request_id: &str, body: RenderContext) -> Turn {
    host(render(request_id, body))
}

fn stop(request_id: &str) -> Turn {
    host(stop_render(request_id))
}

fn press(subscription: &str, body: RenderContext, action_id: &str) -> Turn {
    host(action(subscription, body, action_id))
}

#[test]
fn chat_mode() {
    check(
        "chat",
        Mode::Chat,
        vec![
            Turn::Start,
            handshake_ok(),
            bot_refused(),
            room_created(),
            message_posted(),
            open("h:1", message()),
            open("h:2", card()),
            press("w:4", message(), BUMP_ACTION),
            press("w:4", message(), "other"),
            Turn::Suspend,
            Turn::Resume,
            stray_reply(),
            host(vec![0xff]),
            stop("h:1"),
            Turn::Resume,
        ],
    );
}

#[test]
fn chat_mode_with_a_refused_room() {
    check(
        "chat-room-refused",
        Mode::Chat,
        vec![
            Turn::Start,
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
        Mode::Chat,
        vec![
            Turn::Start,
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
        Mode::Pocket,
        vec![
            Turn::Start,
            handshake_ok(),
            open("h:7", card()),
            open("h:8", message()),
            press("w:2", card(), BUMP_ACTION),
            Turn::Suspend,
            Turn::Resume,
            stop("h:7"),
            press("w:2", card(), BUMP_ACTION),
        ],
    );
}

#[test]
fn unified_mode() {
    check(
        "unified",
        Mode::Unified,
        vec![
            Turn::Start,
            handshake_ok(),
            bot_registered(),
            room_created(),
            message_posted(),
            open("h:1", message()),
            open("h:2", card()),
            press("w:4", card(), BUMP_ACTION),
            press("w:4", message(), BUMP_ACTION),
            Turn::Suspend,
            Turn::Resume,
            stop("h:2"),
            press("w:4", message(), BUMP_ACTION),
        ],
    );
}
