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
