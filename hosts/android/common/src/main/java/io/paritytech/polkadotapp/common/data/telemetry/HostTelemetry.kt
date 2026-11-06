package io.paritytech.polkadotapp.common.data.telemetry

import android.content.Intent

data class HostEventOrigin(
    val appVariant: String,
    val appFlavor: String,
    val nativeAppVersion: String,
    val platform: String,
    val osVersion: String,
    val appBuild: Long,
)

/**
 * One host event. Sentry search cannot filter or chart extras, so anything to query by is a tag; a step
 * with a [durationMs] is also sent as a distribution under [name], carrying the tags as attributes.
 */
data class HostEvent(
    val name: String,
    val tags: Map<String, String>,
    val extras: Map<String, Any>,
    val durationMs: Long? = null,
)

const val PRODUCT_NOTIFICATION_INTENT_MARKER = "product_notification"
const val PRODUCT_NOTIFICATION_INTENT_PRODUCT_ID = "product_notification_product_id"

fun HostEventOrigin.baseTags(): Map<String, String> = mapOf(
    "host.platform" to platform,
    "host.app_variant" to appVariant,
    "host.app_flavor" to appFlavor,
    "host.native_app_version" to nativeAppVersion,
)

fun HostEventOrigin.baseExtras(): Map<String, Any> = mapOf("host.app_build" to appBuild)

fun HostEventOrigin.productNotificationShown(productId: String) = HostEvent(
    name = "host.product_notification.shown",
    tags = baseTags() + ("product_notification.product_id" to productId),
    extras = baseExtras(),
)

fun HostEventOrigin.productNotificationTapped(productId: String) = HostEvent(
    name = "host.product_notification.tapped",
    tags = baseTags() + ("product_notification.product_id" to productId),
    extras = baseExtras(),
)

/** A tap is recorded once: not again when the activity is restored or relaunched from Recents. */
fun isFreshNotificationLaunch(isRestored: Boolean, intentFlags: Int): Boolean =
    !isRestored && (intentFlags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) == 0

enum class HostEventOutcome(val tag: String) {
    OK("ok"),
    ERROR("error"),
    CACHE_HIT("cache_hit"),
    TIMEOUT("timeout"),
    NO_WORKER("no_worker"),
    ABANDONED("abandoned"),
}

fun HostEventOrigin.stepEvent(
    name: String,
    outcome: HostEventOutcome,
    monotonicMs: Long,
    durationMs: Long,
    sessionId: String?,
    tags: Map<String, String> = emptyMap(),
    extras: Map<String, Any> = emptyMap(),
) = HostEvent(
    name = name,
    tags = baseTags() + ("outcome" to outcome.tag) + sessionId.sessionTag() + tags,
    extras = baseExtras() +
        mapOf("event.monotonic_ms" to monotonicMs, "duration_ms" to durationMs) +
        sessionId.sessionTag() +
        extras,
    durationMs = durationMs,
)

// The join key with the product's own events, which carry it as a tag. Kept as an extra too, where
// the events sent before it was a tag have it.
private fun String?.sessionTag(): Map<String, String> = this?.let { mapOf("host.session_id" to it) }.orEmpty()

fun interface HostEventOriginProvider {
    fun current(): HostEventOrigin
}

fun interface HostEventCapture {
    /** False while host telemetry is off, so work done only to report an event can be skipped. */
    val enabled: Boolean get() = true

    fun capture(event: () -> HostEvent)
}

fun interface HostMonotonicTimeProvider {
    fun nowMillis(): Long
}
