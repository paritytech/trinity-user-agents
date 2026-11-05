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

data class HostEvent(
    val name: String,
    val tags: Map<String, String>,
    val extras: Map<String, Any>,
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

fun interface HostEventOriginProvider {
    fun current(): HostEventOrigin
}

fun interface HostEventCapture {
    fun capture(event: () -> HostEvent)
}

fun interface HostMonotonicTimeProvider {
    fun nowMillis(): Long
}
