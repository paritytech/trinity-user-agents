use parity_scale_codec::{Decode, Encode};

/// Push notification payload.
///
/// When `scheduled_at` is `Some`, the notification is deferred to the given
/// wall-clock instant (Unix milliseconds UTC). `None` fires immediately,
/// preserving prior behaviour. See [RFC 0019].
///
/// [RFC 0019]: https://github.com/paritytech/host-rust-core/blob/main/docs/rfcs/0019-scheduled-notifications.md
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
