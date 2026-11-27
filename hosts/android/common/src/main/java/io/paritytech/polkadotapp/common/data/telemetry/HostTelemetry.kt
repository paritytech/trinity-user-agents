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

enum class HostEventLevel {
    INFO,
    ERROR,
}

data class HostEvent(
    val name: String,
    val tags: Map<String, String>,
    val extras: Map<String, Any>,
    val level: HostEventLevel,
)

/** A flow that broke. It is grouped on its flow and kind, and only the type of [error] leaves the device. */
data class CriticalIssue(
    val flow: String,
    val kind: String,
    val error: Throwable?,
)

fun HostEventOrigin.baseTags(): Map<String, String> = mapOf(
    "host.platform" to platform,
    "host.app_variant" to appVariant,
    "host.app_flavor" to appFlavor,
    "host.native_app_version" to nativeAppVersion,
)

fun HostEventOrigin.baseExtras(): Map<String, Any> = mapOf("host.app_build" to appBuild)

/** A tap is recorded once: not again when the activity is restored or relaunched from Recents. */
fun isFreshNotificationLaunch(isRestored: Boolean, intentFlags: Int): Boolean =
    !isRestored && (intentFlags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) == 0

fun interface HostEventOriginProvider {
    fun current(): HostEventOrigin
}

fun interface HostEventCapture {
    fun capture(event: () -> HostEvent)
}

fun interface CriticalIssueCapture {
    fun capture(issue: CriticalIssue)
}

fun interface HostMonotonicTimeProvider {
    fun nowMillis(): Long
}
