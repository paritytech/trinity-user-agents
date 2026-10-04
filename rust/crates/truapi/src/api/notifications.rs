//! Unified [`Notifications`] trait.

use crate::versioned::notifications::{
    HostPushNotificationCancelError, HostPushNotificationCancelRequest,
    HostPushNotificationCancelResponse, HostPushNotificationError, HostPushNotificationRequest,
    HostPushNotificationResponse,
    HostNotificationReceiverStatusRequest, HostNotificationReceiverStatusResponse,
    HostNotificationReplaceReceiverRequest, HostNotificationReplaceReceiverResponse,
    HostNotificationDisableReceiverRequest, HostNotificationDisableReceiverResponse,
    HostNotificationRecordReceiptRequest, HostNotificationRecordReceiptResponse,
    HostNotificationReceiverEventsRequest, HostNotificationReceiverEventsResponse,
    HostNotificationAcknowledgeReceiverEventRequest, HostNotificationAcknowledgeReceiverEventResponse,
    HostNotificationReceivingError,
};
use crate::{CallContext, CallError};
use crate::{wire, wire_trait};

/// Local notification scheduling and consent-scoped background receiving.
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
    /// [RFC 0019]: https://github.com/paritytech/trinity-user-agents/blob/main/docs/rfcs/0019-scheduled-notifications.md
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
    /// [RFC 0019]: https://github.com/paritytech/trinity-user-agents/blob/main/docs/rfcs/0019-scheduled-notifications.md
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

    /// Inspect current host support, consent and durable registration state.
    ///
    /// ```ts
    /// const result = await truapi.notifications.receiverStatus();
    /// assert(result.isOk(), "receiverStatus failed:", result);
    /// console.log(result.value);
    /// ```
    #[wire(id = 2)]
    async fn receiver_status(
        &self,
        cx: &CallContext,
        request: HostNotificationReceiverStatusRequest,
    ) -> Result<HostNotificationReceiverStatusResponse, CallError<HostNotificationReceivingError>>;

    /// Atomically replace watches under explicit receiving consent.
    ///
    /// ```ts
    /// const status = await truapi.notifications.receiverStatus();
    /// assert(status.isOk(), "receiverStatus failed:", status);
    /// // An empty policy removes every watch; nonempty policies require scoped consent.
    /// const result = await truapi.notifications.replaceReceiver({
    ///   expectedRevision: status.value.revision,
    ///   watches: [],
    /// });
    /// assert(result.isOk(), "replaceReceiver failed:", result);
    /// ```
    #[wire(id = 3)]
    async fn replace_receiver(
        &self,
        cx: &CallContext,
        request: HostNotificationReplaceReceiverRequest,
    ) -> Result<HostNotificationReplaceReceiverResponse, CallError<HostNotificationReceivingError>>;

    /// Disable locally and queue transport revocation without waiting for it.
    ///
    /// ```ts
    /// const status = await truapi.notifications.receiverStatus();
    /// assert(status.isOk(), "receiverStatus failed:", status);
    /// const result = await truapi.notifications.disableReceiver({
    ///   expectedRevision: status.value.revision,
    /// });
    /// assert(result.isOk(), "disableReceiver failed:", result);
    /// ```
    #[wire(id = 4)]
    async fn disable_receiver(
        &self,
        cx: &CallContext,
        request: HostNotificationDisableReceiverRequest,
    ) -> Result<HostNotificationDisableReceiverResponse, CallError<HostNotificationReceivingError>>;

    /// Record foreground handling, reading or actual OS display, and return the
    /// confirmed/pending display outcome. A reservation is not proof of display.
    ///
    /// ```ts
    /// const events = await truapi.notifications.receiverEvents({ afterSequence: 0n });
    /// assert(events.isOk(), "receiverEvents failed:", events);
    /// const event = events.value[0];
    /// if (event) {
    ///   const result = await truapi.notifications.recordReceipt({
    ///     revision: event.revision, watchId: event.watchId,
    ///     eventId: event.eventId, kind: "Foreground",
    ///   });
    ///   assert(result.isOk(), "recordReceipt failed:", result);
    ///   console.log(result.value);
    /// }
    /// ```
    #[wire(id = 5)]
    async fn record_receipt(
        &self,
        cx: &CallContext,
        request: HostNotificationRecordReceiptRequest,
    ) -> Result<HostNotificationRecordReceiptResponse, CallError<HostNotificationReceivingError>>;

    /// Poll bounded durable delivery and activation events.
    ///
    /// ```ts
    /// const result = await truapi.notifications.receiverEvents({ afterSequence: 0n });
    /// assert(result.isOk(), "receiverEvents failed:", result);
    /// console.log(result.value);
    /// ```
    #[wire(id = 6)]
    async fn receiver_events(
        &self,
        cx: &CallContext,
        request: HostNotificationReceiverEventsRequest,
    ) -> Result<HostNotificationReceiverEventsResponse, CallError<HostNotificationReceivingError>>;

    /// Acknowledge an event after application handling.
    ///
    /// ```ts
    /// const events = await truapi.notifications.receiverEvents({ afterSequence: 0n });
    /// assert(events.isOk(), "receiverEvents failed:", events);
    /// for (const event of events.value) {
    ///   console.log("Received event:", event.kind, event.watchId);
    ///   const result = await truapi.notifications.acknowledgeReceiverEvent({
    ///     sequence: event.sequence,
    ///   });
    ///   assert(result.isOk(), "acknowledgeReceiverEvent failed:", result);
    /// }
    /// ```
    #[wire(id = 7)]
    async fn acknowledge_receiver_event(
        &self,
        cx: &CallContext,
        request: HostNotificationAcknowledgeReceiverEventRequest,
    ) -> Result<HostNotificationAcknowledgeReceiverEventResponse, CallError<HostNotificationReceivingError>>;
}
