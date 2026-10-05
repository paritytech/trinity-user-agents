//! Unified [`Notifications`] trait.

use crate::versioned::notifications::{
    HostPushNotificationCancelError, HostPushNotificationCancelRequest,
    HostPushNotificationCancelResponse, HostPushNotificationError, HostPushNotificationRequest,
    HostPushNotificationResponse,
    NotificationActivationAcknowledgeError, NotificationActivationAcknowledgeRequest,
    NotificationActivationAcknowledgeResponse, NotificationActivationEventsError,
    NotificationActivationEventsRequest, NotificationActivationEventsResponse,
};
use crate::{CallContext, CallError};
use crate::{wire, wire_trait};

/// Notification methods for locally-rendered push notifications.
#[wire_trait(id = 8)]
#[crate::async_trait]
pub trait Notifications: Send + Sync {
    /// Send a push notification to the user.
    ///
    /// Returns a notification id that can be
    /// passed to [`cancel_push_notification`](Self::cancel_push_notification)
    /// to retract a scheduled notification. When `scheduled_at` is set the host
    /// persists the notification across restarts and fires it through the
    /// platform-native scheduler. See [RFC 0019].
    ///
    /// [RFC 0019]: https://github.com/paritytech/host-rust-core/blob/main/docs/rfcs/0019-scheduled-notifications.md
    ///
    /// ```ts
    /// const result = await truapi.notifications.sendPushNotification({
    ///   text: "Hello!",
    /// });
    /// assert(result.isOk(), "sendPushNotification failed:", result);
    /// console.log("notification sent:", result.value);
    /// ```
    #[wire(id = 0)]
    async fn send_push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>>;

    /// Cancels a previously issued push notification.
    ///
    /// Cancellation is idempotent: returns `Ok(())` whether the notification is
    /// still pending, already fired, or was never issued. See [RFC 0019].
    ///
    /// [RFC 0019]: https://github.com/paritytech/host-rust-core/blob/main/docs/rfcs/0019-scheduled-notifications.md
    ///
    /// ```ts
    /// const result = await truapi.notifications.cancelPushNotification({
    ///   id: 1,
    /// });
    /// assert(result.isOk(), "cancelPushNotification failed:", result);
    /// console.log("notification cancelled");
    /// ```
    #[wire(id = 1)]
    async fn cancel_push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationCancelRequest,
    ) -> Result<HostPushNotificationCancelResponse, CallError<HostPushNotificationCancelError>>;

    /// Retrieve up to 32 pending activations for this runtime's authenticated
    /// product, account and environment. Retrieval does not consume events,
    /// prompt for permissions or enroll a background receiver.
    ///
    /// ```ts
    /// const result = await truapi.notifications.activationEvents();
    /// assert(result.isOk(), "activationEvents failed:", result);
    /// console.log("pending activations:", result.value);
    /// ```
    #[wire(id = 8)]
    async fn activation_events(
        &self,
        cx: &CallContext,
        request: NotificationActivationEventsRequest,
    ) -> Result<NotificationActivationEventsResponse, CallError<NotificationActivationEventsError>>;

    /// Acknowledge exactly one activation after the product router handles it.
    /// Unknown or already acknowledged sequences are idempotent, and can never
    /// remove an activation from another product, account or environment.
    ///
    /// ```ts
    /// const result = await truapi.notifications.acknowledgeActivation({ sequence: 1n });
    /// assert(result.isOk(), "acknowledgeActivation failed:", result);
    /// ```
    #[wire(id = 9)]
    async fn acknowledge_activation(
        &self,
        cx: &CallContext,
        request: NotificationActivationAcknowledgeRequest,
    ) -> Result<NotificationActivationAcknowledgeResponse, CallError<NotificationActivationAcknowledgeError>>;
}
