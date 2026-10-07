//! The runner's evidence: one JSON object per line, to stdout and optionally
//! to a file. Frames are decoded with the host's own envelope codec and named
//! from the generated wire table, so the transcript reads as method names
//! rather than byte pairs.

use std::path::Path;

use anyhow::{Context, Result};
use parity_scale_codec::Decode;
use tokio::io::AsyncWriteExt;
use truapi::frame::{
    MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE, MESSAGE_TYPE_STOP,
    ProtocolMessage,
};
use truapi::generated::wire_table::{WIRE_TABLE, WireKind};

/// Which way a frame crossed.
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    /// The host sent it to the guest.
    HostToGuest,
    /// The guest sent it to the host.
    GuestToHost,
}

/// Sink for transcript lines.
pub struct Transcript {
    file: Option<tokio::fs::File>,
}

impl Transcript {
    /// Open `path` for appending, if given. The file is truncated first so a
    /// run never reads an earlier run's lines as its own.
    pub async fn open(path: Option<&Path>) -> Result<Self> {
        let file = match path {
            Some(path) => Some(
                tokio::fs::File::create(path)
                    .await
                    .with_context(|| format!("create {}", path.display()))?,
            ),
            None => None,
        };
        Ok(Self { file })
    }

    /// Record one frame with its decoded address.
    pub async fn frame(&mut self, direction: Direction, bytes: &[u8]) -> Result<()> {
        let line = match ProtocolMessage::decode(&mut &bytes[..]) {
            Ok(message) => {
                let (method, kind) = name_of(message.payload.trait_id, message.payload.method_id);
                serde_json::json!({
                    "kind": "frame",
                    "direction": match direction {
                        Direction::HostToGuest => "host->guest",
                        Direction::GuestToHost => "guest->host",
                    },
                    "requestId": message.request_id,
                    "method": method,
                    "leg": leg_name(kind, message.payload.message_type),
                    "traitId": message.payload.trait_id,
                    "methodId": message.payload.method_id,
                    "payloadBytes": message.payload.value.len(),
                })
            }
            Err(error) => serde_json::json!({
                "kind": "frame",
                "direction": format!("{direction:?}"),
                "undecodable": error.to_string(),
                "bytes": bytes.len(),
            }),
        };
        self.write(line).await
    }

    /// Record one line the guest logged.
    pub async fn guest(&mut self, line: &str) -> Result<()> {
        self.write(serde_json::json!({ "kind": "guest", "event": line }))
            .await
    }

    /// Record a runner-side note.
    pub async fn note(&mut self, line: serde_json::Value) -> Result<()> {
        self.write(line).await
    }

    async fn write(&mut self, line: serde_json::Value) -> Result<()> {
        let text = format!("{line}\n");
        tokio::io::stdout().write_all(text.as_bytes()).await?;
        if let Some(file) = self.file.as_mut() {
            file.write_all(text.as_bytes()).await?;
            file.flush().await?;
        }
        Ok(())
    }
}

/// The wire-table name and kind for a `(trait, method)` pair.
fn name_of(trait_id: u8, method_id: u8) -> (&'static str, Option<&'static WireKind>) {
    WIRE_TABLE
        .iter()
        .find(|entry| {
            let ids = match &entry.kind {
                WireKind::Request(ids) | WireKind::Subscription(ids) => ids,
            };
            ids.trait_id == trait_id && ids.method_id == method_id
        })
        .map_or(("unknown", None), |entry| (entry.method, Some(&entry.kind)))
}

fn leg_name(kind: Option<&WireKind>, message_type: u8) -> &'static str {
    match (kind, message_type) {
        (Some(WireKind::Request(_)), MESSAGE_TYPE_REQUEST) => "request",
        (Some(WireKind::Request(_)), MESSAGE_TYPE_RESPONSE) => "response",
        (Some(WireKind::Subscription(_)), MESSAGE_TYPE_REQUEST) => "start",
        (Some(WireKind::Subscription(_)), MESSAGE_TYPE_RESPONSE) => "receive",
        (Some(WireKind::Subscription(_)), MESSAGE_TYPE_INTERRUPT) => "interrupt",
        (Some(WireKind::Subscription(_)), MESSAGE_TYPE_STOP) => "stop",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    //! The guest hardcodes wire discriminants because the protocol-only build
    //! has no wire table. These pin them to the generated table so a renumbered
    //! method fails here, not on a live host.

    use truapi::frame::{request_ids, subscription_ids};
    use wasm_worker_probe_guest::wire;

    fn request(method: &str) -> (u8, u8) {
        let ids = request_ids(method).unwrap_or_else(|| panic!("{method} is a request"));
        (ids.trait_id, ids.method_id)
    }

    fn subscription(method: &str) -> (u8, u8) {
        let ids = subscription_ids(method).unwrap_or_else(|| panic!("{method} is a subscription"));
        (ids.trait_id, ids.method_id)
    }

    #[test]
    fn guest_discriminants_match_the_generated_wire_table() {
        assert_eq!(
            [
                wire::SYSTEM_HANDSHAKE,
                wire::CHAT_CREATE_ROOM,
                wire::CHAT_REGISTER_BOT,
                wire::CHAT_POST_MESSAGE,
                wire::RENDERER_RENDER,
                wire::RENDERER_ACTION_SUBSCRIBE,
            ],
            [
                request("system_handshake"),
                request("chat_create_room"),
                request("chat_register_bot"),
                request("chat_post_message"),
                subscription("renderer_render"),
                subscription("renderer_action_subscribe"),
            ]
        );
    }

    #[test]
    fn guest_message_types_match_the_frame_codec() {
        use truapi::frame::{
            MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_RESPONSE, MESSAGE_TYPE_STOP,
        };
        assert_eq!(
            [
                wire::MESSAGE_TYPE_REQUEST,
                wire::MESSAGE_TYPE_RESPONSE,
                wire::MESSAGE_TYPE_INTERRUPT,
                wire::MESSAGE_TYPE_STOP
            ],
            [
                MESSAGE_TYPE_REQUEST,
                MESSAGE_TYPE_RESPONSE,
                MESSAGE_TYPE_INTERRUPT,
                MESSAGE_TYPE_STOP
            ]
        );
    }
}
