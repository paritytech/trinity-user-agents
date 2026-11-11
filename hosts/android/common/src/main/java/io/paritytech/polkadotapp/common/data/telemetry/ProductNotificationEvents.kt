package io.paritytech.polkadotapp.common.data.telemetry

enum class ProductNotificationStep(val value: String) {
    /** The alarm is registered and the booking is stored. */
    SCHEDULED("scheduled"),

    /** The booking failed, and the event carries the reason. */
    NOT_SCHEDULED("not_scheduled"),

    /**
     * The host posted the notification, in any process state.
     *
     * The iOS host sends this step only when a notification fires in the foreground. Its next sweep then reports
     * every fired booking, in the foreground or not, as `delivered` while the notification is still in Notification
     * Center and as `gone` once it has left, so a count of `shown` is not comparable across hosts.
     */
    SHOWN("shown"),

    /** The notification was due and nothing reached the person, and the event carries the reason. */
    SUPPRESSED("suppressed"),

    /** The host withdrew a stored booking. */
    CANCELLED("cancelled"),

    /** The person tapped the notification. */
    TAPPED("tapped"),
    ;

    val eventName: String get() = "host.product_notification.$value"
}

enum class ProductNotificationReason(val value: String) {
    LIMIT_REACHED("limit_reached"),
    ID_COLLISION("id_collision"),
    ADD_FAILED("add_failed"),
    STORE_FAILED("store_failed"),
    NOTIFICATIONS_DISABLED("notifications_disabled"),
    CHANNEL_BLOCKED("channel_blocked"),
    BOOKING_MISSING("booking_missing"),
}

/**
 * Builds one step of a product notification trail, naming the product and the booking id only.
 *
 * The notification text and deeplink have no parameter here, so they cannot reach an event. The id is tagged as
 * the unsigned number the product receives.
 */
fun HostEventOrigin.productNotification(
    step: ProductNotificationStep,
    productId: String,
    notificationId: Int?,
    reason: ProductNotificationReason?,
    scheduledAtMillis: Long?,
    deliveredAtMillis: Long?,
): HostEvent {
    val tags = buildMap {
        putAll(baseTags())
        put("product_notification.product_id", productId)
        notificationId?.let { put("product_notification.id", it.toUInt().toString()) }
        reason?.let { put("product_notification.reason", it.value) }
    }
    val extras = buildMap<String, Any> {
        putAll(baseExtras())
        scheduledAtMillis?.let { put("product_notification.scheduled_at_unix_ms", it) }
        deliveredAtMillis?.let { put("product_notification.delivered_at_unix_ms", it) }
    }
    return HostEvent(name = step.eventName, tags = tags, extras = extras)
}
