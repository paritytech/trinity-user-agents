package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.telemetry.CriticalIssue
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventLevel
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.baseExtras
import io.paritytech.polkadotapp.common.data.telemetry.baseTags

const val PRODUCT_NOTIFICATION_INTENT_MARKER = "product_notification"
const val PRODUCT_NOTIFICATION_INTENT_PRODUCT_ID = "product_notification_product_id"
const val PRODUCT_NOTIFICATION_INTENT_NOTIFICATION_ID = "product_notification_id"

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

/** [issueKind] is the [CriticalIssue] kind of a broken booking, null for a booking the host refused or held back. */
enum class ProductNotificationReason(val value: String, val issueKind: String?) {
    LIMIT_REACHED("limit_reached", issueKind = null),
    ID_COLLISION("id_collision", issueKind = "id-collision"),
    ADD_FAILED("add_failed", issueKind = "add-failed"),
    STORE_FAILED("store_failed", issueKind = "store-failed"),
    READ_FAILED("read_failed", issueKind = "read-failed"),
    NOTIFICATIONS_DISABLED("notifications_disabled", issueKind = null),
    CHANNEL_BLOCKED("channel_blocked", issueKind = null),
    BOOKING_MISSING("booking_missing", issueKind = null),
}

/**
 * Builds one step of a product notification trail, naming the product and the booking id only.
 *
 * The notification text and deeplink have no parameter here, so they cannot reach an event. The id is sent as
 * the unsigned number the product receives.
 */
internal fun HostEventOrigin.productNotification(
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
        reason?.let { put("product_notification.reason", it.value) }
    }
    val extras = buildMap<String, Any> {
        putAll(baseExtras())
        notificationId?.let { put("product_notification.id", it.toUInt().toLong()) }
        scheduledAtMillis?.let { put("product_notification.scheduled_at_unix_ms", it) }
        deliveredAtMillis?.let { put("product_notification.delivered_at_unix_ms", it) }
    }
    val level = if (reason?.issueKind == null) HostEventLevel.INFO else HostEventLevel.ERROR
    return HostEvent(name = step.eventName, tags = tags, extras = extras, level = level)
}
