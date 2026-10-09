//! Typed backend errors and JSON-RPC error synthesis for dropped requests.

#[cfg(feature = "smoldot")]
use serde_json::{Value, json};

#[cfg(feature = "smoldot")]
use crate::storage::StorageClientError;

/// Failure modes of a [`ChainProvider`](crate::platform::ChainProvider) and of
/// the embedded provider's own API, so callers can match on the cause (e.g.
/// for retry or telemetry).
///
/// Which variants are constructed depends on the enabled backends and target
/// (e.g. `MissingRuntime` is native-WebSocket only), so the enum as a whole
/// allows dead variants rather than cfg-gating each one.
#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum ProviderError {
    /// No backend is registered — and no bundled network defines — this
    /// genesis hash.
    #[display("no chain registered for genesis 0x{}", hex::encode(genesis))]
    UnknownGenesis {
        /// The queried genesis hash.
        genesis: [u8; 32],
    },
    /// A parachain named a relay that is not a registered light-client chain.
    #[display(
        "relay 0x{} is not a registered light-client chain",
        hex::encode(relay)
    )]
    UnknownRelay {
        /// The relay genesis hash the parachain referenced.
        relay: [u8; 32],
    },
    /// The WebSocket handshake with a remote node failed.
    #[display("WebSocket handshake with {url} failed: {reason}")]
    Handshake {
        /// The node URL.
        url: String,
        /// The underlying failure.
        reason: String,
    },
    /// smoldot rejected the chain spec when adding the chain.
    #[display("failed to add a chain to the light client: {reason}")]
    AddChain {
        /// The underlying failure.
        reason: String,
    },
    /// The light client already holds as many connections as the provider
    /// allows.
    #[display("the light client already holds {limit} connections")]
    TooManyConnections {
        /// The ceiling that was reached.
        limit: usize,
    },
    /// The embedded light client is not running this chain, so there is no
    /// lifecycle to watch until something connects to it. A chain served by a
    /// remote node never has one.
    #[display(
        "the light client is not running chain 0x{}; connect to it first, since only light-client chains have a lifecycle",
        hex::encode(genesis)
    )]
    NotRunning {
        /// The queried genesis hash.
        genesis: [u8; 32],
    },
    /// The native WebSocket backend was called without an ambient tokio
    /// runtime to drive its transport.
    #[display("the WebSocket backend requires an ambient tokio runtime")]
    MissingRuntime,
    /// A transport-level failure (e.g. browser WebSocket creation).
    #[display("{reason}")]
    Transport {
        /// The underlying failure.
        reason: String,
    },
    /// A host's own [`ChainProvider`](crate::platform::ChainProvider)
    /// implementation refused or failed to open the connection.
    #[display("{reason}")]
    Host {
        /// The reason the host reported.
        reason: String,
    },
    /// Warm start was asked of a provider built without a store.
    #[display(
        "this provider was built without storage, so there is nowhere to keep finalized state"
    )]
    NoStorage,
    /// The warm store could not read or write a blob.
    #[cfg(feature = "smoldot")]
    #[display("{_0}")]
    Storage(StorageClientError),
    /// No bundled network has this name.
    #[display("unknown network \"{name}\"; bundled: {known}")]
    UnknownNetwork {
        /// The requested name.
        name: String,
        /// Comma-separated names of the bundled networks.
        known: String,
    },
    /// A bundled genesis hash does not decode to 32 bytes.
    #[display("bundled genesis hash {hex} is malformed")]
    MalformedGenesis {
        /// The catalog entry as written.
        hex: String,
    },
}

#[cfg(feature = "smoldot")]
impl From<StorageClientError> for ProviderError {
    fn from(error: StorageClientError) -> Self {
        Self::Storage(error)
    }
}

/// `url` without its userinfo, for an error that will be logged.
///
/// [`ProviderError::Handshake`] prints the URL in its `Display`, so a node
/// address carrying `user:pass@` would otherwise leak the credentials into
/// every log line and error report that touches it.
#[cfg(feature = "ws")]
pub fn redacted(url: &url::Url) -> String {
    if url.username().is_empty() && url.password().is_none() {
        return url.to_string();
    }

    // Both setters reject only a URL with no host, an empty domain, or the
    // `file` scheme. None of those can carry userinfo, so any URL reaching here
    // has already returned above and the discarded results cannot hide a
    // credential.
    let mut stripped = url.clone();
    let _ = stripped.set_username("");
    let _ = stripped.set_password(None);
    stripped.to_string()
}

/// JSON-RPC internal-error code (per the spec's reserved range).
#[cfg(feature = "smoldot")]
const JSON_RPC_INTERNAL_ERROR: i32 = -32603;

/// Build a JSON-RPC error response echoing `request`'s id, or `None` when the
/// request carries no id (a notification, which expects no response) or is not
/// valid JSON.
///
/// The light backend calls this when smoldot refuses a request (a full queue):
/// the connection stays alive, and the consumer correlates responses by id, so
/// without a synthesized error for that id it would wait forever. (The
/// WebSocket backends instead end the whole response stream, since a send
/// failure there means the socket is dead.)
#[cfg(feature = "smoldot")]
pub fn synthetic_error_frame(request: &str, message: &str) -> Option<String> {
    let value: Value = serde_json::from_str(request).ok()?;
    let id = value.get("id").filter(|id| !id.is_null())?;
    Some(
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": JSON_RPC_INTERNAL_ERROR, "message": message },
        })
        .to_string(),
    )
}

/// Outcome of matching a JSON-RPC response frame against an awaited request id.
#[cfg(feature = "smoldot")]
pub enum FrameForId {
    /// The frame answers `id` with a string `result`.
    Result(String),
    /// The frame answers `id` with an `error`, or with a `result` that is not a
    /// string.
    Failure(String),
}

/// Match `frame` against the awaited `id`, returning how it answers.
///
/// `None` means the frame belongs to some other request (or is not a response),
/// so the caller keeps waiting. Both answer cases must terminate the wait: a
/// node that rejects the method answers with an `error` and keeps the socket
/// open, so treating an error frame as unmatched would wait forever.
#[cfg(feature = "smoldot")]
pub fn frame_for_id(frame: &str, id: &str) -> Option<FrameForId> {
    let value: Value = serde_json::from_str(frame).ok()?;
    if value.get("id")?.as_str()? != id {
        return None;
    }
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Some(FrameForId::Failure(message.to_owned()));
    }
    match value.get("result").and_then(Value::as_str) {
        Some(result) => Some(FrameForId::Result(result.to_owned())),
        None => {
            Some(FrameForId::Failure(
                "response carried no string result".to_owned(),
            ))
        }
    }
}

#[cfg(all(test, feature = "smoldot"))]
mod tests {
    use super::{FrameForId, frame_for_id, synthetic_error_frame};

    #[test]
    fn a_matching_result_frame_yields_its_string() {
        let frame = frame_for_id(r#"{"id":"db","result":"blob"}"#, "db").expect("the id matches");
        let FrameForId::Result(result) = frame else {
            panic!("expected a result");
        };
        assert_eq!(result, "blob");
    }

    #[test]
    fn a_matching_error_frame_fails_instead_of_waiting() {
        // A node that does not implement the method answers with an error and
        // keeps the socket open, so this must terminate the caller's wait.
        let frame = frame_for_id(
            r#"{"id":"db","error":{"code":-32601,"message":"Method not found"}}"#,
            "db",
        )
        .expect("the id matches");
        let FrameForId::Failure(reason) = frame else {
            panic!("expected a failure");
        };
        assert_eq!(reason, "Method not found");
    }

    #[test]
    fn a_matching_non_string_result_fails() {
        let frame = frame_for_id(r#"{"id":"db","result":null}"#, "db").expect("the id matches");
        assert!(matches!(frame, FrameForId::Failure(_)));
    }

    #[test]
    fn other_ids_and_notifications_are_not_matched() {
        assert!(frame_for_id(r#"{"id":"other","result":"blob"}"#, "db").is_none());
        assert!(frame_for_id(r#"{"method":"chainHead_v1_followEvent"}"#, "db").is_none());
        assert!(frame_for_id("not json", "db").is_none());
    }

    #[test]
    fn echoes_the_request_id() {
        let frame = synthetic_error_frame(
            r#"{"jsonrpc":"2.0","id":7,"method":"x","params":[]}"#,
            "queue full",
        )
        .expect("a request with an id yields an error frame");
        let value: serde_json::Value = serde_json::from_str(&frame).expect("valid JSON");
        assert_eq!(value["id"], 7);
        assert_eq!(value["error"]["code"], -32603);
        assert_eq!(value["error"]["message"], "queue full");
    }

    #[test]
    fn string_ids_are_preserved() {
        let frame =
            synthetic_error_frame(r#"{"id":"abc","method":"x"}"#, "boom").expect("has an id");
        let value: serde_json::Value = serde_json::from_str(&frame).expect("valid JSON");
        assert_eq!(value["id"], "abc");
    }

    #[test]
    fn notifications_and_garbage_yield_nothing() {
        assert!(synthetic_error_frame(r#"{"method":"x","params":[]}"#, "m").is_none());
        assert!(synthetic_error_frame(r#"{"id":null,"method":"x"}"#, "m").is_none());
        assert!(synthetic_error_frame("not json", "m").is_none());
    }
}

#[cfg(all(test, feature = "ws"))]
mod ws_tests {
    use super::redacted;

    #[test]
    fn userinfo_is_stripped_from_a_node_url() {
        let url = url::Url::parse("wss://alice:hunter2@node.example/path").expect("parse");

        let printed = redacted(&url);

        assert!(!printed.contains("alice"), "leaked the username: {printed}");
        assert!(
            !printed.contains("hunter2"),
            "leaked the password: {printed}"
        );
        assert!(printed.contains("node.example"), "lost the host: {printed}");
    }

    #[test]
    fn a_username_without_a_password_is_stripped_too() {
        let url = url::Url::parse("wss://token@node.example/").expect("parse");

        assert!(!redacted(&url).contains("token"));
    }

    /// A URL with no userinfo is the common case and must survive verbatim, so
    /// the diagnostic keeps its path and port.
    #[test]
    fn a_url_without_credentials_is_unchanged() {
        let url = url::Url::parse("wss://node.example:9944/ws").expect("parse");

        assert_eq!(redacted(&url), url.to_string());
    }
}
