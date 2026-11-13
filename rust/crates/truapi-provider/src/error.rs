//! Typed backend errors and JSON-RPC error synthesis for dropped requests.

#[cfg(feature = "smoldot")]
use serde_json::{Value, json};

#[cfg(feature = "smoldot")]
use crate::storage::StorageClientError;

/// Failure modes of a [`ChainProvider`](crate::platform::ChainProvider) and of
/// the embedded provider's own API, so callers can match on the cause (e.g.
/// for retry or telemetry).
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
	#[display("relay 0x{} is not a registered light-client chain", hex::encode(relay))]
	UnknownRelay {
		/// The relay genesis hash the parachain referenced.
		relay: [u8; 32],
	},
	/// The WebSocket handshake with a remote node failed.
	#[display("WebSocket handshake with {url} failed: {reason}")]
	Handshake {
		/// The node's scheme, host and port, without its path or query.
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

/// `url` reduced to its scheme, host and port, for an error that will be logged.
///
/// [`ProviderError::Handshake`] prints the URL in its `Display`. A node address
/// can carry a credential as `user:pass@`, in its path, or in its query under
/// any parameter name, so only the part that names the node is kept.
#[cfg(feature = "ws")]
pub fn redacted(url: &url::Url) -> String {
	url.origin().ascii_serialization()
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
		let message = error.get("message").and_then(Value::as_str).unwrap_or("unknown error");
		return Some(FrameForId::Failure(message.to_owned()));
	}
	match value.get("result").and_then(Value::as_str) {
		Some(result) => Some(FrameForId::Result(result.to_owned())),
		None => Some(FrameForId::Failure("response carried no string result".to_owned())),
	}
}

#[cfg(all(test, feature = "smoldot"))]
mod tests;

#[cfg(all(test, feature = "ws"))]
mod ws_tests {
	use super::redacted;

	#[test]
	fn userinfo_is_stripped_from_a_node_url() {
		let url = url::Url::parse("wss://alice:hunter2@node.example/path").expect("parse");

		let printed = redacted(&url);

		assert!(!printed.contains("alice"), "leaked the username: {printed}");
		assert!(!printed.contains("hunter2"), "leaked the password: {printed}");
		assert!(printed.contains("node.example"), "lost the host: {printed}");
	}

	#[test]
	fn a_username_without_a_password_is_stripped_too() {
		let url = url::Url::parse("wss://token@node.example/").expect("parse");

		assert!(!redacted(&url).contains("token"));
	}

	/// Providers put API keys in the path or under query names of their own, so
	/// neither survives, while the host and port still name the node.
	#[test]
	fn the_path_and_query_are_dropped() {
		let url =
			url::Url::parse("wss://node.example:9944/v1/PATHKEY?apikey=SECRET").expect("parse");

		assert_eq!(redacted(&url), "wss://node.example:9944");
	}
}
