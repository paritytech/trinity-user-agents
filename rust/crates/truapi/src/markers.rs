//! Every marker the core addresses to a host, and whether it may leave the
//! device.
//!
//! A marker is a dotted target such as `truapi.chain.follow_budget_reached`.
//! The core raises it, [`crate::logging`] hands it to the host, and the host
//! decides what to do with it by looking it up here rather than by matching a
//! prefix of its own. A target this table does not list is
//! [`MarkerClass::Local`]: it stays on the device until someone says what it
//! carries.
//!
//! Each entry's doc comment says what its detail may contain. That is the
//! prose a reviewer reads when deciding whether a marker may be exported, and
//! it is the reason the class is a property of the marker rather than of the
//! call site.

/// Whether a marker may leave the device, and at what severity if it may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerClass {
	/// Stays on the device. Its detail may name what the person is doing.
	Local,
	/// May be shipped off-device, at the given severity.
	Export(Severity),
}

/// How loud an exported marker is once it reaches a host's logger.
///
/// The core raises most markers without a level, so this is assigned here
/// rather than read from the event, and it is what lets a core error be
/// alerted on separately from a core warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
	/// Routine progress. Useful as context around a failure, not on its own.
	Info,
	/// Something degraded that the core recovered from or worked around.
	Warn,
	/// Something failed that the core could not recover from.
	Error,
}

/// One marker, and what its detail is allowed to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
	/// The dotted target, as raised by the core.
	pub target: &'static str,
	/// Whether the marker may leave the device.
	pub class: MarkerClass,
}

/// Builds a [`Marker`] whose detail never leaves the device.
const fn local(target: &'static str) -> Marker {
	Marker { target, class: MarkerClass::Local }
}

/// Builds a [`Marker`] whose detail may be exported at `severity`.
const fn export(target: &'static str, severity: Severity) -> Marker {
	Marker { target, class: MarkerClass::Export(severity) }
}

// ---------------------------------------------------------------- core

/// The core finished starting. Detail carries its version string.
pub const CORE_STARTED: Marker = export("truapi.core.started", Severity::Info);

/// A chain refused another follow because the node's budget is spent. Detail
/// names the chain and the budget, never an account or a call.
pub const CHAIN_FOLLOW_BUDGET_REACHED: Marker =
	export("truapi.chain.follow_budget_reached", Severity::Warn);

/// A `tracing` warning raised anywhere in the crate. Detail is the event's own
/// message, which is written for operators and names no person.
pub const TRACING_WARN: Marker = export("truapi.tracing.warn", Severity::Warn);

/// A `tracing` error raised anywhere in the crate. Detail is the event's own
/// message, which is written for operators and names no person.
pub const TRACING_ERROR: Marker = export("truapi.tracing.error", Severity::Error);

// ------------------------------------------------- native host callbacks
//
// These stay on the device. Their details describe what the person is doing
// right now: the page they are opening, the notification they are being shown,
// the permission they are being asked for.

/// The host was asked to open a URL. Detail carries that URL.
pub const NATIVE_NAVIGATE_TO: Marker = local("truapi.native.callback.navigate_to");

/// The host was asked to raise a notification. Detail carries its body.
pub const NATIVE_PUSH_NOTIFICATION: Marker = local("truapi.native.callback.push_notification");

/// The host was asked to withdraw a notification. Detail carries its id.
pub const NATIVE_CANCEL_NOTIFICATION: Marker = local("truapi.native.callback.cancel_notification");

/// The host was asked to confirm a permission with the person.
pub const NATIVE_CONFIRM_PERMISSION: Marker = local("truapi.native.callback.confirm_permission");

/// The host was asked to confirm an action with the person.
pub const NATIVE_CONFIRM_USER_ACTION: Marker = local("truapi.native.callback.confirm_user_action");

/// The host was asked for a device permission. Detail names which.
pub const NATIVE_DEVICE_PERMISSION: Marker = local("truapi.native.callback.device_permission");

/// The host reported a device permission's current state.
pub const NATIVE_DEVICE_PERMISSION_STATUS: Marker =
	local("truapi.native.callback.device_permission_status");

/// The host was asked to confirm a remote permission with the person.
pub const NATIVE_REMOTE_PERMISSION: Marker = local("truapi.native.callback.remote_permission");

/// The host was asked whether it supports a feature. Detail names it.
pub const NATIVE_FEATURE_SUPPORTED: Marker = local("truapi.native.callback.feature_supported");

/// The host was asked which chains it supports.
pub const NATIVE_SUPPORTED_CHAINS: Marker = local("truapi.native.callback.supported_chains");

/// The host was asked for a chain's transaction extension versions.
pub const NATIVE_TRANSACTION_EXTENSION_VERSIONS: Marker =
	local("truapi.native.callback.transaction_extension_versions");

/// The host reported that auth state changed for the person signed in.
pub const NATIVE_AUTH_STATE_CHANGED: Marker = local("truapi.native.callback.auth_state_changed");

/// A card face was expanded or collapsed. Detail names the card.
pub const NATIVE_SET_EXPANDED_CARD_FACE_SHOWN: Marker =
	local("truapi.native.callback.set_expanded_card_face_shown");

/// The host's chain socket connected. Detail names the chain.
pub const NATIVE_CHAIN_CONNECTED: Marker = local("truapi.native.callback.chain_connected");

/// The host's chain socket is closing. Detail names the chain.
pub const NATIVE_CHAIN_CLOSING: Marker = local("truapi.native.callback.chain_closing");

/// The host could not close a chain socket. Detail carries the host's reason.
pub const NATIVE_CHAIN_CLOSE_FAILED: Marker = local("truapi.native.callback.chain_close_failed");

/// The host could not send on a chain socket. Detail carries the host's reason.
pub const NATIVE_CHAIN_SEND_FAILED: Marker = local("truapi.native.callback.chain_send_failed");

/// Responses already held were served again rather than re-requested.
pub const NATIVE_CHAIN_RESPONSES_REUSED: Marker = local("truapi.native.chain.responses_reused");

/// The host runtime booted. Detail carries its configuration.
pub const NATIVE_HOST_RUNTIME_BOOT: Marker = local("truapi.native.host_runtime.boot");

/// A preimage was found in the store. Detail carries its key.
pub const NATIVE_PREIMAGE_FOUND: Marker = local("truapi.native.preimage.found");

/// A preimage lookup is being retried. Detail carries its key.
pub const NATIVE_PREIMAGE_RETRYING: Marker = local("truapi.native.preimage.retrying");

// ------------------------------------------------------------ ws bridge
//
// The local WebSocket bridge a product's web view talks to. Details carry
// ports, counts and socket errors, never product payloads.

/// The bridge's listener came up. Detail carries the port and runtime id.
pub const WS_BRIDGE_STARTED: Marker = export("truapi.ws_bridge.started", Severity::Info);

/// The bridge's accept loop is exiting.
pub const WS_BRIDGE_SHUTDOWN: Marker = export("truapi.ws_bridge.shutdown", Severity::Info);

/// A connection was accepted. Detail carries the count against the limit.
pub const WS_BRIDGE_CONNECTION_OPEN: Marker =
	export("truapi.ws_bridge.connection_open", Severity::Info);

/// A connection closed. Detail carries the count against the limit.
pub const WS_BRIDGE_CONNECTION_CLOSED: Marker =
	export("truapi.ws_bridge.connection_closed", Severity::Info);

/// The listener was rebound after being lost. Detail carries the new port.
pub const WS_BRIDGE_RELISTENED: Marker = export("truapi.ws_bridge.relistened", Severity::Info);

/// A requested bind port was ignored because the listener was already up.
pub const WS_BRIDGE_BIND_PORT_IGNORED: Marker =
	export("truapi.ws_bridge.bind_port_ignored", Severity::Warn);

/// A text frame was dropped because the bridge speaks binary.
pub const WS_BRIDGE_TEXT_FRAME_IGNORED: Marker =
	export("truapi.ws_bridge.text_frame_ignored", Severity::Warn);

/// One execution reached its connection limit.
pub const WS_BRIDGE_CONNECTION_LIMIT_EXECUTION: Marker =
	export("truapi.ws_bridge.connection_limit_execution", Severity::Warn);

/// The bridge reached its total connection limit across executions.
pub const WS_BRIDGE_CONNECTION_LIMIT_TOTAL: Marker =
	export("truapi.ws_bridge.connection_limit_total", Severity::Warn);

/// A half-finished handshake was evicted to make room.
pub const WS_BRIDGE_HANDSHAKE_BACKLOG_EVICTED: Marker =
	export("truapi.ws_bridge.handshake_backlog_evicted", Severity::Warn);

/// A handshake ran out of time.
pub const WS_BRIDGE_HANDSHAKE_TIMEOUT: Marker =
	export("truapi.ws_bridge.handshake_timeout", Severity::Warn);

/// A connection was refused because its token did not match.
pub const WS_BRIDGE_REJECT_UNAUTHORIZED: Marker =
	export("truapi.ws_bridge.reject_unauthorized", Severity::Warn);

/// An execution was revoked while one of its connections was still setting up.
pub const WS_BRIDGE_CONNECTION_REVOKED_DURING_SETUP: Marker =
	export("truapi.ws_bridge.connection_revoked_during_setup", Severity::Warn);

/// The listener could not accept. Detail carries the socket error.
pub const WS_BRIDGE_ACCEPT_ERROR: Marker = export("truapi.ws_bridge.accept_error", Severity::Error);

/// A frame could not be decoded. Detail carries the protocol error.
pub const WS_BRIDGE_FRAME_ERROR: Marker = export("truapi.ws_bridge.frame_error", Severity::Error);

/// A handshake failed. Detail carries the reason.
pub const WS_BRIDGE_HANDSHAKE_ERROR: Marker =
	export("truapi.ws_bridge.handshake_error", Severity::Error);

/// A read failed. Detail carries the socket error.
pub const WS_BRIDGE_READ_ERROR: Marker = export("truapi.ws_bridge.read_error", Severity::Error);

/// A send failed. Detail carries the socket error.
pub const WS_BRIDGE_SEND_ERROR: Marker = export("truapi.ws_bridge.send_error", Severity::Error);

/// The listener was lost and the socket was dropped to free its port.
pub const WS_BRIDGE_LISTENER_LOST: Marker =
	export("truapi.ws_bridge.listener_lost", Severity::Error);

/// The listener could not be rebound. Detail carries the reason.
pub const WS_BRIDGE_RELISTEN_FAILED: Marker =
	export("truapi.ws_bridge.relisten_failed", Severity::Error);

/// Every marker the core addresses to a host.
///
/// A host reads this once and matches a target exactly. Anything missing is
/// [`MarkerClass::Local`], so a marker added later stays on the device until
/// it is listed here.
pub const MARKERS: &[Marker] = &[
	CORE_STARTED,
	CHAIN_FOLLOW_BUDGET_REACHED,
	TRACING_WARN,
	TRACING_ERROR,
	NATIVE_NAVIGATE_TO,
	NATIVE_PUSH_NOTIFICATION,
	NATIVE_CANCEL_NOTIFICATION,
	NATIVE_CONFIRM_PERMISSION,
	NATIVE_CONFIRM_USER_ACTION,
	NATIVE_DEVICE_PERMISSION,
	NATIVE_DEVICE_PERMISSION_STATUS,
	NATIVE_REMOTE_PERMISSION,
	NATIVE_FEATURE_SUPPORTED,
	NATIVE_SUPPORTED_CHAINS,
	NATIVE_TRANSACTION_EXTENSION_VERSIONS,
	NATIVE_AUTH_STATE_CHANGED,
	NATIVE_SET_EXPANDED_CARD_FACE_SHOWN,
	NATIVE_CHAIN_CONNECTED,
	NATIVE_CHAIN_CLOSING,
	NATIVE_CHAIN_CLOSE_FAILED,
	NATIVE_CHAIN_SEND_FAILED,
	NATIVE_CHAIN_RESPONSES_REUSED,
	NATIVE_HOST_RUNTIME_BOOT,
	NATIVE_PREIMAGE_FOUND,
	NATIVE_PREIMAGE_RETRYING,
	WS_BRIDGE_STARTED,
	WS_BRIDGE_SHUTDOWN,
	WS_BRIDGE_CONNECTION_OPEN,
	WS_BRIDGE_CONNECTION_CLOSED,
	WS_BRIDGE_RELISTENED,
	WS_BRIDGE_BIND_PORT_IGNORED,
	WS_BRIDGE_TEXT_FRAME_IGNORED,
	WS_BRIDGE_CONNECTION_LIMIT_EXECUTION,
	WS_BRIDGE_CONNECTION_LIMIT_TOTAL,
	WS_BRIDGE_HANDSHAKE_BACKLOG_EVICTED,
	WS_BRIDGE_HANDSHAKE_TIMEOUT,
	WS_BRIDGE_REJECT_UNAUTHORIZED,
	WS_BRIDGE_CONNECTION_REVOKED_DURING_SETUP,
	WS_BRIDGE_ACCEPT_ERROR,
	WS_BRIDGE_FRAME_ERROR,
	WS_BRIDGE_HANDSHAKE_ERROR,
	WS_BRIDGE_READ_ERROR,
	WS_BRIDGE_SEND_ERROR,
	WS_BRIDGE_LISTENER_LOST,
	WS_BRIDGE_RELISTEN_FAILED,
];

/// The class of `target`, or `None` when the table does not list it.
pub fn class_of(target: &str) -> Option<MarkerClass> {
	MARKERS.iter().find(|marker| marker.target == target).map(|marker| marker.class)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_target_is_listed_once() {
		let mut targets: Vec<_> = MARKERS.iter().map(|marker| marker.target).collect();
		let total = targets.len();
		targets.sort_unstable();
		targets.dedup();
		assert_eq!(targets.len(), total, "a target is listed twice");
	}

	#[test]
	fn every_target_is_host_addressed() {
		for marker in MARKERS {
			assert!(
				marker.target.starts_with(crate::logging::HOST_MARKER_PREFIX),
				"{} is not addressed to a host",
				marker.target
			);
		}
	}

	#[test]
	fn an_unlisted_target_has_no_class() {
		assert_eq!(class_of("truapi.not.registered"), None);
		assert_eq!(class_of(CORE_STARTED.target), Some(CORE_STARTED.class));
	}

	/// Host-addressed targets a `tracing` macro raises, which must be string
	/// literals and so cannot use the constants above.
	fn tracing_targets(source: &str) -> Vec<String> {
		source
			.match_indices("target: \"")
			.filter_map(|(at, matched)| {
				let rest = &source[at + matched.len()..];
				rest.find('"').map(|end| rest[..end].to_string())
			})
			.filter(|target| target.starts_with(crate::logging::HOST_MARKER_PREFIX))
			.collect()
	}

	#[test]
	fn every_tracing_macro_target_is_registered() {
		let sources = [("chain_runtime.rs", include_str!("chain_runtime.rs"))];
		for (name, source) in sources {
			let targets = tracing_targets(source);
			assert!(!targets.is_empty(), "{name} raises no host-addressed target; drop it here");
			for target in targets {
				assert!(
					class_of(&target).is_some(),
					"{name} raises {target}, which is not registered"
				);
			}
		}
	}

	#[test]
	fn the_targets_the_layer_builds_are_registered() {
		for level in ["warn", "error"] {
			let target = format!("truapi.tracing.{level}");
			assert!(
				class_of(&target).is_some(),
				"the layer raises {target}, which is not registered"
			);
		}
	}

	#[test]
	fn native_callbacks_stay_on_the_device() {
		for marker in MARKERS.iter().filter(|m| m.target.starts_with("truapi.native.")) {
			assert_eq!(marker.class, MarkerClass::Local, "{} would be exported", marker.target);
		}
	}
}
