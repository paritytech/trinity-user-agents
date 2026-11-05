package io.paritytech.polkadotapp.common.data.telemetry

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

fun HostEventOrigin.baseTags(): Map<String, String> = mapOf(
    "host.platform" to platform,
    "host.app_variant" to appVariant,
    "host.app_flavor" to appFlavor,
    "host.native_app_version" to nativeAppVersion,
)

fun HostEventOrigin.baseExtras(): Map<String, Any> = mapOf("host.app_build" to appBuild)

enum class HostEventOutcome(val tag: String) {
    OK("ok"),
    ERROR("error"),
    CACHE_HIT("cache_hit"),
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
    tags = baseTags() + ("outcome" to outcome.tag) + tags,
    extras = baseExtras() +
        mapOf("event.monotonic_ms" to monotonicMs, "duration_ms" to durationMs) +
        sessionId?.let { mapOf("host.session_id" to it) }.orEmpty() +
        extras,
)

fun interface HostEventOriginProvider {
    fun current(): HostEventOrigin
}

fun interface HostEventCapture {
    fun capture(event: () -> HostEvent)
}

fun interface HostMonotonicTimeProvider {
    fun nowMillis(): Long
}
