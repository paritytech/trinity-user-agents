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

	// ------------------------------------------------------- funding facts
	//
	// One `CoreEvent` per fact of a funding session, built in
	// `runtime/funding_telemetry.rs`. Each carries its own attributes, then
	// `funding.ref`, a random label drawn per session and kept in memory
	// only, the session's labels and its `age_ms`. Every value is a label
	// from a closed set, a condition, or a count or milliseconds; never an
	// intent, a quote, transaction or reference id, an address, an exact
	// amount, fee or limit, a provider's own text, or a URL. Below, a key
	// with no values listed is a condition, and one ending in `_ms` or naming
	// a number is a count, left off when unknown.
	//
	// The session's labels: `direction` (`in`, `out`), `opener` (`host`,
	// `product`), `product` (a first-party product's dotNS label,
	// `third_party`, `none`), `rail` (`card`, `bank`, `crypto`, `none`),
	// `provider` (a provider the host lists, `other`, `none`), `asset` and
	// `network` (a known id a provider route declares, such as `usdt` or
	// `polkadot-asset-hub`, else `other`; `none`), `amount_bucket` (CASH: `<10`, `10-50`, `50-100`, `100-250`,
	// `250-500`, `500-1k`, `1k-5k`, `5k+`, `unset`). A session the core no
	// longer holds carries `session` as `unknown` instead, and no age.

	/// The overlay opened on a new session. `amount_preset`: `true` when the
	/// opener named the amount.
	FUNDING_OVERLAY_OPENED = "truapi.funding.overlay_opened" => Export(Info);

	/// The overlay closed. `outcome`: `started`, `dismissed`, `failed`.
	/// `furthest`, how far the user got: `amount`, `rail`, `quotes`,
	/// `provider`. `age_ms` is how long it was up.
	FUNDING_OVERLAY_CLOSED = "truapi.funding.overlay_closed" => Export(Info);

	/// Every provider asked for one quote request answered, or a newer
	/// request replaced it first. `region`: `eu`, `uk`, `us`, `other`,
	/// `unset`. `issue`: `none`, `not_enough_balance`, `below_minimum`,
	/// `above_maximum`. `superseded`. Counts `asked`, `quoted`,
	/// `refused_country`, `refused_min`, `refused_max`,
	/// `refused_unavailable`, `refused_other`, `timed_out`, `pending`;
	/// `first_quote_ms` and `settle_ms` from the request.
	FUNDING_QUOTES_SETTLED = "truapi.funding.quotes_settled" => Export(Info);

	/// The session was handed to a provider on one of its quotes. `best`.
	/// `fee_bucket`, fees against what the user sends in basis points:
	/// `<50`, `50-100`, `100-200`, `200-500`, `500+`. `eta_bucket`: `<1m`,
	/// `1-10m`, `10-60m`, `1-24h`, `1d+`, `unknown`. `rank` by what the user
	/// gets, and `quote_count`.
	FUNDING_PROVIDER_CHOSEN = "truapi.funding.provider_chosen" => Export(Info);

	/// The provider's worker took the session, or first failed to.
	/// `outcome`: `taken`, `not_taken`. `take_ms` since it was handed over.
	FUNDING_PROVIDER_TOOK = "truapi.funding.provider_took" => Export(Info);

	/// The session first reached a milestone. `step`: `received`, `added`,
	/// `sent`. `since_prev_ms` since the milestone before.
	FUNDING_STEP_REACHED = "truapi.funding.step_reached" => Export(Info);

	/// The session first showed a warning state. `flag`: `taking_longer`,
	/// `retrying`, `retrying_payout`.
	FUNDING_STATUS_FLAG = "truapi.funding.status_flag" => Export(Info);

	/// The provider is done with the session, on an ending that is no fault.
	/// `outcome`: `topped_up`, `sent`, `refunded`, `payout_failed`, `failed`,
	/// `expired`, `cancelled`, `paid_out_after_cancel`. `reason`: `none`,
	/// `refunded`, `payout_failed`, a failure's kind such as `declined`, the
	/// core's `not_claimed` or `payment_failed`, or `provider_other`.
	/// `payout_failed` comes only under `ended_unexpectedly`. `money_moved`,
	/// `region`, and `to_received_ms`; `age_ms` is the session's whole run.
	FUNDING_ENDED = "truapi.funding.ended" => Export(Info);

	/// An ending that is a fault: failed, a failed payout, or expired once
	/// money moved, and not the user's or the rules' doing. Carries what
	/// `truapi.funding.ended` does.
	FUNDING_ENDED_UNEXPECTEDLY = "truapi.funding.ended_unexpectedly" => Export(Error);

	/// The user asked to cancel. `result`: `ended`, `requested`, `pending`,
	/// `too_late`, `refused`. `status_at`: the status variant when asked,
	/// such as `waiting_for_payment`. `money_moved`.
	FUNDING_CANCEL_REQUESTED = "truapi.funding.cancel_requested" => Export(Info);

	/// The provider answered a cancel, or its worker first failed to.
	/// `answer`: `offered`, `refused`, `stopped`, `no_answer`. `fee_bucket`:
	/// an offer's fee against its total, as on `provider_chosen`, or `none`.
	/// `answer_ms` since the cancel was asked.
	FUNDING_CANCEL_ANSWERED = "truapi.funding.cancel_answered" => Export(Info);

	/// The user answered a cancel offer. `result`: `accepted`, `declined`,
	/// `lapsed`, `no_offer`, `too_late`.
	FUNDING_CANCEL_CONFIRMED = "truapi.funding.cancel_confirmed" => Export(Info);

	/// The provider asked to show a screen. `target`: `app`, `page`.
	/// `replaced`: whether it replaced one still waiting.
	FUNDING_FRAME_REQUESTED = "truapi.funding.frame_requested" => Export(Info);

	/// The host opened a screen the provider asked for. `target`, and
	/// `wait_ms` since it was asked for.
	FUNDING_FRAME_OPENED = "truapi.funding.frame_opened" => Export(Info);

	/// A provider's screen closed. `target`. `outcome`: `closed`,
	/// `dismissed`, `returned` or `failed` once shown, `closed_by_core` when
	/// withdrawn before it showed, `dropped_unopened` when the user never
	/// opened it. `open_ms` on screen.
	FUNDING_FRAME_CLOSED = "truapi.funding.frame_closed" => Export(Info);

	// ------------------------------------------------------------- materials

	/// A `Materials` node draws its children plain. Detail names the node, the
	/// material and the reason. It stays on the device: a node that falls back is
	/// the product's to hear about, not a crash, and with materials off every
	/// node does.
	MATERIALS_NODE_PLAIN = "truapi.materials.node_plain" => Local;

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

	/// An expanded card was turned to its back or its front.
	NATIVE_SET_EXPANDED_CARD_BACK_SHOWN = "truapi.native.callback.set_expanded_card_back_shown" => Local;

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
