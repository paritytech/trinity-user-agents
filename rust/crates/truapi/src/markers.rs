//! Every marker the core addresses to a host, and whether it may leave the
//! device.
//!
//! A marker is a dotted target such as `truapi.chain.follow_budget_reached`.
//! The core raises it, the host sink hands it to the host, and the host
//! decides what to do with it by looking it up here rather than by matching a
//! prefix of its own. A host-addressed target this table does not list has no
//! class, and nothing downstream knows what its detail carries. Every call
//! site names a constant from here, so an unlisted target cannot be written by
//! accident, and the tracing layer and the bridge both refuse one they cannot
//! find in the table.
//!
//! Each entry's doc comment says what its detail may contain. That is the
//! prose a reviewer reads when deciding whether a marker may be exported, and
//! it is the reason the class is a property of the marker rather than of the
//! call site.

/// Prefix that marks an event's target as addressed to the host rather than
/// naming a module. Rust module paths separate with `::`, so a dotted target
/// cannot collide with one, and the rest of the target names the condition.
pub const HOST_MARKER_PREFIX: &str = "truapi.";

/// Whether a marker may leave the device, and at what severity if it may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MarkerClass {
	/// Stays on the device. Its detail may name what the person is doing.
	Local,
	/// May be shipped off-device, at the given severity.
	Export {
		/// How loud the marker is once a host's logger has it.
		severity: Severity,
	},
}

/// How loud an exported marker is once it reaches a host's logger.
///
/// The core raises most markers without a level, so this is assigned here
/// rather than read from the event, and it is what lets a core error be
/// alerted on separately from a core warning. Its `&'static str` form is the
/// lowercase name the generated marker table uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
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

/// Declares every marker once.
///
/// Each entry gives a constant, the dotted target it carries, and its class:
/// `Local`, or `Export(Info)`, `Export(Warn)`, `Export(Error)`. From that one
/// list this emits the constant, its entry in [`MARKERS`], and its arm in
/// [`class_of`], so a marker cannot be declared without being listed.
macro_rules! markers {
	($(
		$(#[$entry:meta])*
		$name:ident = $target:literal => $class:ident $(($severity:ident))?;
	)*) => {
		$(
			$(#[$entry])*
			pub const $name: Marker = Marker {
				target: $target,
				class: MarkerClass::$class $({ severity: Severity::$severity })?,
			};
		)*

		/// Every marker the core addresses to a host, grouped by subsystem.
		///
		/// A host reads this once and matches a target exactly. A marker the core
		/// raises without listing here is refused on its way out, so forgetting
		/// this table costs the line rather than exporting it.
		pub const MARKERS: &[Marker] = &[$($name,)*];

		/// The class of `target`, or `None` when the table does not list it.
		///
		/// Two entries sharing a target leave the second arm unreachable, which
		/// the compiler rejects.
		pub fn class_of(target: &str) -> Option<MarkerClass> {
			match target {
				$($target => Some(MarkerClass::$class $({ severity: Severity::$severity })?),)*
				_ => None,
			}
		}
	};
}

markers! {
	// ---------------------------------------------------------------- core

	/// The core finished starting. Detail carries its version string.
	CORE_STARTED = "truapi.core.started" => Export(Info);

	/// A chain refused another follow because the node's budget is spent. Detail
	/// names the chain and the budget, never an account or a call.
	CHAIN_FOLLOW_BUDGET_REACHED = "truapi.chain.follow_budget_reached" => Export(Warn);

	/// A `tracing` warning raised anywhere in the crate. Detail is the event's own
	/// message, which is written for operators and names no person.
	TRACING_WARN = "truapi.tracing.warn" => Export(Warn);

	/// A `tracing` error raised anywhere in the crate. Detail is the event's own
	/// message, which is written for operators and names no person.
	TRACING_ERROR = "truapi.tracing.error" => Export(Error);

	// ------------------------------------------------------------- funding
	//
	// Health of the funding modality. No detail names a session, a quote, an
	// amount or an account; a reason a host gave is bounded.
	// Funding's flow facts are the markers after these.

	/// The expiry sweep could not store its result. Detail carries the
	/// storage error.
	FUNDING_SWEEP_FAILED = "truapi.funding.sweep_failed" => Export(Warn);

	/// Loading the stored sessions failed and is retried. Detail carries the
	/// storage error and the pause before the retry.
	FUNDING_SESSIONS_LOAD_FAILED = "truapi.funding.sessions_load_failed" => Export(Warn);

	/// The stored sessions were written by a newer app, so funding stays
	/// unavailable. Detail carries the stored format number.
	FUNDING_NEWER_FORMAT = "truapi.funding.newer_format" => Export(Error);

	/// A stored session record does not decode, or is of a format this app
	/// cannot upgrade. Detail names the table, the format and the size.
	FUNDING_RECORD_UNREADABLE = "truapi.funding.record_unreadable" => Export(Error);

	/// Storing a session's outcome once its funds moved failed, and is
	/// retried. Detail carries the storage error and the pause.
	FUNDING_SETTLE_FAILED = "truapi.funding.settle_failed" => Export(Warn);

	/// Storing a provider's answer to a cancel failed. Detail carries the
	/// storage error.
	FUNDING_CANCEL_ANSWER_STORE_FAILED = "truapi.funding.cancel_answer_store_failed" => Export(Warn);

	/// A provider answered a cancel with something it may not, which counts
	/// as a refusal. Detail names the answer's kind only.
	FUNDING_CANCEL_ANSWER_INVALID = "truapi.funding.cancel_answer_invalid" => Export(Error);

	/// Asking the host whether it holds funds a provider started failed.
	/// Detail carries the host's reason, bounded.
	FUNDING_FUNDS_READ_FAILED = "truapi.funding.funds_read_failed" => Export(Warn);

	/// Storing whether the host holds those funds failed. Detail carries the
	/// storage error.
	FUNDING_FUNDS_STORE_FAILED = "truapi.funding.funds_store_failed" => Export(Warn);

	/// Reading a top-up's or payment's status from the host failed. Detail
	/// carries the host's reason, bounded.
	FUNDING_STATUS_READ_FAILED = "truapi.funding.status_read_failed" => Export(Warn);

	/// A session's funds cannot be followed, as the host installed no top-up
	/// or payment platform.
	FUNDING_PLATFORM_MISSING = "truapi.funding.platform_missing" => Export(Error);

	/// The overlay could not be shown or its session stored. Detail names the
	/// error's kind only.
	FUNDING_OVERLAY_FAILED = "truapi.funding.overlay_failed" => Export(Warn);

	/// A session armed to start on its best quote did not. Detail names the
	/// error's kind only.
	FUNDING_AUTO_START_FAILED = "truapi.funding.auto_start_failed" => Export(Warn);

	/// The host's funding provider configuration names something that is not
	/// a product. Detail carries that name, from the host's own config.
	FUNDING_PROVIDER_CONFIG_INVALID = "truapi.funding.provider_config_invalid" => Export(Warn);

	/// Storing what providers' quotes showed they serve failed. Detail
	/// carries the storage error.
	FUNDING_SUPPORT_STORE_FAILED = "truapi.funding.support_store_failed" => Export(Warn);

	/// Reading what providers' quotes showed they serve failed, or the
	/// records did not decode. Detail carries the error.
	FUNDING_SUPPORT_READ_FAILED = "truapi.funding.support_read_failed" => Export(Warn);

	/// The launch marker that resumes funding was not written. Detail carries
	/// the I/O error, not the path.
	FUNDING_RESUME_MARKER_FAILED = "truapi.funding.resume_marker_failed" => Export(Error);

	// ------------------------------------------------------- funding facts
	//
	// The funding flow, one line per fact. Each detail is
	// `funding.ref=<ref> key=value ...`: a random per-session reference, then
	// only the keys `crate::funding_events` lists for the marker, every value
	// from a closed set, amounts as buckets and a country as its region.

	/// The funding overlay opened on a new session. Detail:
	/// `crate::funding_events::OVERLAY_OPENED`.
	FUNDING_OVERLAY_OPENED = "truapi.funding.overlay_opened" => Export(Info);

	/// The funding overlay closed, and how far the user got. Detail:
	/// `crate::funding_events::OVERLAY_CLOSED`.
	FUNDING_OVERLAY_CLOSED = "truapi.funding.overlay_closed" => Export(Info);

	/// Every provider asked for one quote request answered, or a newer request replaced it. Detail:
	/// `crate::funding_events::QUOTES_SETTLED`.
	FUNDING_QUOTES_SETTLED = "truapi.funding.quotes_settled" => Export(Info);

	/// A session was handed to a provider on one of its quotes. Detail:
	/// `crate::funding_events::PROVIDER_CHOSEN`.
	FUNDING_PROVIDER_CHOSEN = "truapi.funding.provider_chosen" => Export(Info);

	/// A provider's worker took its session, or first failed to. Detail:
	/// `crate::funding_events::PROVIDER_TOOK`.
	FUNDING_PROVIDER_TOOK = "truapi.funding.provider_took" => Export(Info);

	/// A session first reached a milestone. Detail: `crate::funding_events::STEP_REACHED`.
	FUNDING_STEP_REACHED = "truapi.funding.step_reached" => Export(Info);

	/// A session first showed a warning state. Detail: `crate::funding_events::STATUS_FLAG`.
	FUNDING_STATUS_FLAG = "truapi.funding.status_flag" => Export(Info);

	/// A provider is done with a session, on an ending that is no fault. Detail:
	/// `crate::funding_events::ENDED`.
	FUNDING_ENDED = "truapi.funding.ended" => Export(Info);

	/// A provider is done with a session, on an ending that is a fault. Detail:
	/// `crate::funding_events::ENDED_UNEXPECTEDLY`.
	FUNDING_ENDED_UNEXPECTEDLY = "truapi.funding.ended_unexpectedly" => Export(Error);

	/// The user asked to cancel a session. Detail: `crate::funding_events::CANCEL_REQUESTED`.
	FUNDING_CANCEL_REQUESTED = "truapi.funding.cancel_requested" => Export(Info);

	/// A provider answered a cancel, or its worker first failed to. Detail:
	/// `crate::funding_events::CANCEL_ANSWERED`.
	FUNDING_CANCEL_ANSWERED = "truapi.funding.cancel_answered" => Export(Info);

	/// The user answered a provider's cancel offer. Detail:
	/// `crate::funding_events::CANCEL_CONFIRMED`.
	FUNDING_CANCEL_CONFIRMED = "truapi.funding.cancel_confirmed" => Export(Info);

	/// A provider asked to show a screen. Detail: `crate::funding_events::FRAME_REQUESTED`.
	FUNDING_FRAME_REQUESTED = "truapi.funding.frame_requested" => Export(Info);

	/// The host opened a screen a provider asked for. Detail:
	/// `crate::funding_events::FRAME_OPENED`.
	FUNDING_FRAME_OPENED = "truapi.funding.frame_opened" => Export(Info);

	/// A provider's screen closed, or was dropped before the user opened it. Detail:
	/// `crate::funding_events::FRAME_CLOSED`.
	FUNDING_FRAME_CLOSED = "truapi.funding.frame_closed" => Export(Info);

	// ------------------------------------------------- native host callbacks
	//
	// These stay on the device. Their details describe what the person is doing
	// right now: the page they are opening, the notification they are being shown,
	// the permission they are being asked for.

	/// The host was asked to open a URL. Detail carries that URL.
	NATIVE_NAVIGATE_TO = "truapi.native.callback.navigate_to" => Local;

	/// The host was asked to raise a notification. Detail carries its body.
	NATIVE_PUSH_NOTIFICATION = "truapi.native.callback.push_notification" => Local;

	/// The host was asked to withdraw a notification. Detail carries its id.
	NATIVE_CANCEL_NOTIFICATION = "truapi.native.callback.cancel_notification" => Local;

	/// The host was asked to confirm a permission with the person.
	NATIVE_CONFIRM_PERMISSION = "truapi.native.callback.confirm_permission" => Local;

	/// The host was asked to confirm an action with the person.
	NATIVE_CONFIRM_USER_ACTION = "truapi.native.callback.confirm_user_action" => Local;

	/// The host was asked for a device permission. Detail names which.
	NATIVE_DEVICE_PERMISSION = "truapi.native.callback.device_permission" => Local;

	/// The host reported a device permission's current state.
	NATIVE_DEVICE_PERMISSION_STATUS = "truapi.native.callback.device_permission_status" => Local;

	/// The host was asked to confirm a remote permission with the person.
	NATIVE_REMOTE_PERMISSION = "truapi.native.callback.remote_permission" => Local;

	/// The host was asked whether it supports a feature. Detail names it.
	NATIVE_FEATURE_SUPPORTED = "truapi.native.callback.feature_supported" => Local;

	/// The host was asked for a chain's transaction extension versions.
	NATIVE_TRANSACTION_EXTENSION_VERSIONS = "truapi.native.callback.transaction_extension_versions" => Local;

	/// The host reported that auth state changed for the person signed in.
	NATIVE_AUTH_STATE_CHANGED = "truapi.native.callback.auth_state_changed" => Local;

	/// A card face was expanded or collapsed. Detail names the card.
	NATIVE_SET_EXPANDED_CARD_FACE_SHOWN = "truapi.native.callback.set_expanded_card_face_shown" => Local;

	/// The host's chain socket connected. Detail names the chain.
	NATIVE_CHAIN_CONNECTED = "truapi.native.callback.chain_connected" => Local;

	/// The host's chain socket is closing. Detail names the chain.
	NATIVE_CHAIN_CLOSING = "truapi.native.callback.chain_closing" => Local;

	/// The host could not close a chain socket. Detail carries the host's reason.
	NATIVE_CHAIN_CLOSE_FAILED = "truapi.native.callback.chain_close_failed" => Local;

	/// The host could not send on a chain socket. Detail carries the host's reason.
	NATIVE_CHAIN_SEND_FAILED = "truapi.native.callback.chain_send_failed" => Local;

	/// Responses already held were served again rather than re-requested.
	NATIVE_CHAIN_RESPONSES_REUSED = "truapi.native.chain.responses_reused" => Local;

	/// The host runtime booted. Detail carries its configuration.
	NATIVE_HOST_RUNTIME_BOOT = "truapi.native.host_runtime.boot" => Local;

	/// A preimage was found in the store. Detail carries its key.
	NATIVE_PREIMAGE_FOUND = "truapi.native.preimage.found" => Local;

	/// A preimage lookup is being retried. Detail carries its key.
	NATIVE_PREIMAGE_RETRYING = "truapi.native.preimage.retrying" => Local;

	// ------------------------------------------------------------ ws bridge
	//
	// The local WebSocket bridge a product's web view talks to. Details carry
	// ports, counts and socket errors, never product payloads. None of these is
	// an error: the bridge recovers from every one of them, by rebinding, by
	// retrying, or by dropping the one connection that failed.

	/// The bridge's listener came up. Detail carries the port and runtime id.
	WS_BRIDGE_STARTED = "truapi.ws_bridge.started" => Export(Info);

	/// The bridge's accept loop is exiting.
	WS_BRIDGE_SHUTDOWN = "truapi.ws_bridge.shutdown" => Export(Info);

	/// A connection was accepted. Detail carries the count against the limit.
	WS_BRIDGE_CONNECTION_OPEN = "truapi.ws_bridge.connection_open" => Export(Info);

	/// A connection closed. Detail carries the count against the limit.
	WS_BRIDGE_CONNECTION_CLOSED = "truapi.ws_bridge.connection_closed" => Export(Info);

	/// The listener was rebound after being lost. Detail carries the new port.
	WS_BRIDGE_RELISTENED = "truapi.ws_bridge.relistened" => Export(Info);

	/// A requested bind port was ignored because the listener was already up.
	WS_BRIDGE_BIND_PORT_IGNORED = "truapi.ws_bridge.bind_port_ignored" => Export(Warn);

	/// A text frame was dropped because the bridge speaks binary.
	WS_BRIDGE_TEXT_FRAME_IGNORED = "truapi.ws_bridge.text_frame_ignored" => Export(Warn);

	/// One execution reached its connection limit.
	WS_BRIDGE_CONNECTION_LIMIT_EXECUTION = "truapi.ws_bridge.connection_limit_execution" => Export(Warn);

	/// The bridge reached its total connection limit across executions.
	WS_BRIDGE_CONNECTION_LIMIT_TOTAL = "truapi.ws_bridge.connection_limit_total" => Export(Warn);

	/// A half-finished handshake was evicted to make room.
	WS_BRIDGE_HANDSHAKE_BACKLOG_EVICTED = "truapi.ws_bridge.handshake_backlog_evicted" => Export(Warn);

	/// A handshake ran out of time.
	WS_BRIDGE_HANDSHAKE_TIMEOUT = "truapi.ws_bridge.handshake_timeout" => Export(Warn);

	/// A connection was refused because its token did not match.
	WS_BRIDGE_REJECT_UNAUTHORIZED = "truapi.ws_bridge.reject_unauthorized" => Export(Warn);

	/// An execution was revoked while one of its connections was still setting up.
	WS_BRIDGE_CONNECTION_REVOKED_DURING_SETUP = "truapi.ws_bridge.connection_revoked_during_setup" => Export(Warn);

	/// The listener could not accept. Detail carries the socket error.
	WS_BRIDGE_ACCEPT_ERROR = "truapi.ws_bridge.accept_error" => Export(Warn);

	/// A frame could not be decoded. Detail carries the protocol error.
	WS_BRIDGE_FRAME_ERROR = "truapi.ws_bridge.frame_error" => Export(Warn);

	/// A handshake failed. Detail carries the reason.
	WS_BRIDGE_HANDSHAKE_ERROR = "truapi.ws_bridge.handshake_error" => Export(Warn);

	/// A read failed. Detail carries the socket error.
	WS_BRIDGE_READ_ERROR = "truapi.ws_bridge.read_error" => Export(Warn);

	/// A send failed. Detail carries the socket error.
	WS_BRIDGE_SEND_ERROR = "truapi.ws_bridge.send_error" => Export(Warn);

	/// The listener was lost and the socket was dropped to free its port.
	WS_BRIDGE_LISTENER_LOST = "truapi.ws_bridge.listener_lost" => Export(Warn);

	/// The listener could not be rebound. Detail carries the reason.
	WS_BRIDGE_RELISTEN_FAILED = "truapi.ws_bridge.relisten_failed" => Export(Warn);
}

#[cfg(test)]
mod tests;
