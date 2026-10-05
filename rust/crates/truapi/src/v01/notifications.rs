use alloc::{string::String, vec::Vec};
use parity_scale_codec::{Decode, Encode};

/// Push notification payload.
///
/// When `scheduled_at` is `Some`, the notification is deferred to the given
/// wall-clock instant (Unix milliseconds UTC). `None` fires immediately,
/// preserving prior behaviour. See [RFC 0019].
///
/// [RFC 0019]: https://github.com/paritytech/trinity-user-agents/blob/main/docs/rfcs/0019-scheduled-notifications.md
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostPushNotificationRequest {
    /// Notification text.
    pub text: String,
    /// Optional URL to open on tap.
    pub deeplink: Option<String>,
    /// Optional Unix timestamp in milliseconds (UTC) at which the notification
    /// should fire. `None` fires immediately.
    pub scheduled_at: Option<u64>,
}

/// Successful push notification response carrying the assigned id.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationResponse {
    /// Host-assigned notification identifier.
    pub id: u32,
}

/// Push notification error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushNotificationError {
    /// The host-wide queue of pending scheduled notifications is full.
    ScheduleLimitReached,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

/// Request to cancel a previously scheduled notification.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationCancelRequest {
    /// The notification identifier returned by [`HostPushNotificationResponse`].
    pub id: u32,
}

/// An authenticated source filter enrolled under host-owned receiving consent.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "runtime", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ReceivingWatch {
    /// Product-local opaque watch identifier.
    pub id: String,
    /// Canonical lowercase 32-byte chain genesis hash.
    pub genesis: String,
    /// Exact source channel, encoded as a canonical lowercase 32-byte hash.
    pub channel: String,
    /// One to four selected topics; every topic must occur in the signed header.
    pub topics: Vec<String>,
    /// Approved Ed25519 public keys in canonical lowercase hex.
    pub senders: Vec<String>,
    /// Expiration in Unix milliseconds, bounded to a JavaScript safe integer.
    pub expires_at: u64,
    /// Unix milliseconds before which delivery is muted; `u64::MAX` means forever.
    pub muted_until: u64,
    /// Product-relative activation route, retained locally and never relayed.
    pub route: String,
}

/// Receiving support, consent and durable synchronization state.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "runtime", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostNotificationReceiverStatus {
    /// Whether this host supplies trusted receiving authority.
    pub supported: bool,
    /// Current OS permission for visible notifications.
    pub os_permission: bool,
    /// Whether current authority and watches have receiving consent.
    pub consent: bool,
    /// Whether receiving is locally enabled.
    pub enabled: bool,
    /// Compare-and-swap token for the durable registration.
    pub revision: u64,
    /// Whether the transport still needs to synchronize this revision.
    pub sync_pending: bool,
    /// Whether the selected platform transport is ready.
    pub transport_ready: bool,
}

/// Confirmed application handling, independent of transport acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "runtime", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ReceivingReceiptKind {
    /// The application handled the event in the foreground; not proof of OS display.
    Foreground,
    /// The user read the event.
    Read,
    /// The product's OS notification API successfully displayed the event.
    Displayed,
}

/// Actual display outcome after recording a receipt, distinct from enrollment ACKs.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "runtime", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostNotificationReceiptResult {
    /// An OS display was positively confirmed by the host or product.
    pub displayed: bool,
    /// A display is reserved but unconfirmed; do not start a competing fallback.
    /// Explicit failure cancels the reservation; unknown outcomes remain pending until expiry.
    pub display_pending: bool,
}

/// Why an authenticated receiving event was queued.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "runtime", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ReceivingEventKind {
    /// An authenticated event arrived without focusing the product.
    Delivery,
    /// The user activated a locally accepted notification.
    Activation,
}

/// A bounded durable event containing opaque identifiers, never plaintext.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "runtime", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ReceivingEvent {
    /// Durable sequence used for polling and acknowledgement.
    pub sequence: u64,
    /// Registration revision that accepted the event.
    pub revision: u64,
    /// Product-local watch identifier.
    pub watch_id: String,
    /// Authenticated event identifier.
    pub event_id: String,
    /// Delivery or user activation.
    pub kind: ReceivingEventKind,
    /// Locally enrolled product-relative route.
    pub route: String,
    /// Expiration in Unix milliseconds.
    pub expires_at: u64,
}

/// Receiving policy, persistence or support failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNotificationReceivingError {
    /// This host has no receiving adapter.
    Unsupported,
    /// Notification permission or scoped receiving consent was denied.
    PermissionDenied,
    /// The request violates the receiving schema or authenticated policy.
    InvalidRequest {
        /// Human-readable reason.
        reason: String,
    },
    /// The registration revision or trusted authority changed.
    Conflict,
    /// The bounded receiving store or watch budget is full.
    Capacity,
    /// Durable persistence failed.
    Storage {
        /// Human-readable reason.
        reason: String,
    },
}

/// Atomically replace the current authority's complete watch set.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNotificationReplaceReceiverRequest {
    /// Revision observed by the caller.
    pub expected_revision: u64,
    /// Complete replacement, at most 256 watches and 10,000 senders total.
    /// Each watch permits at most 1,000 senders.
    pub watches: Vec<ReceivingWatch>,
}

/// Disable locally without awaiting transport revocation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNotificationDisableReceiverRequest {
    /// Revision observed by the caller.
    pub expected_revision: u64,
}

/// Record confirmed foreground handling or reading.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNotificationRecordReceiptRequest {
    /// Revision that accepted the event.
    pub revision: u64,
    /// Product-local watch identifier.
    pub watch_id: String,
    /// Authenticated event identifier.
    pub event_id: String,
    /// Confirmed handling kind.
    pub kind: ReceivingReceiptKind,
}

/// Poll durable events without creating a UI-lifetime subscription.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNotificationReceiverEventsRequest {
    /// Return only events after this sequence.
    pub after_sequence: u64,
}

/// Acknowledge a durable event after application handling.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNotificationAcknowledgeReceiverEventRequest {
    /// Durable event sequence.
    pub sequence: u64,
}

/// A host-admitted notification activation for the authenticated product,
/// account and environment bound to this runtime.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct NotificationActivation {
    /// Host-assigned sequence, unique within the bound activation queue.
    pub sequence: u64,
    /// Identifier of the activated notification.
    pub notification_id: u32,
    /// Validated product-relative route beginning with exactly one slash.
    pub route: String,
}

/// Pending activations, retained until individually acknowledged. Hosts return
/// at most 32 events in sequence order, without consuming them on retrieval.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct NotificationActivations {
    /// Pending events in ascending sequence order.
    pub events: Vec<NotificationActivation>,
}

/// Acknowledge one handled activation in the runtime's bound queue.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct NotificationActivationAcknowledgeRequest {
    /// Exact sequence to acknowledge; never a cumulative watermark.
    pub sequence: u64,
}
