package io.paritytech.polkadotapp.app.telemetry

import android.os.Build
import android.os.SystemClock
import io.paritytech.polkadotapp.app.BuildConfig
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.baseExtras
import io.paritytech.polkadotapp.common.data.telemetry.baseTags
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.sentry.Sentry
import io.sentry.SentryAttribute
import io.sentry.SentryAttributes
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import io.sentry.SentryLogLevel
import io.sentry.logger.SentryLogParameters
import io.sentry.protocol.Message
import java.util.UUID
import javax.inject.Inject

internal fun interface HostTelemetryEnabledProvider {
    fun isEnabled(): Boolean
}

internal class RealHostTelemetryEnabledProvider @Inject constructor() : HostTelemetryEnabledProvider {
    override fun isEnabled(): Boolean = FeatureOption.HOST_TELEMETRY.isEnabled
}

internal fun interface SentryEventCapture {
    fun capture(event: SentryEvent)
}

internal class RealSentryEventCapture @Inject constructor() : SentryEventCapture {
    override fun capture(event: SentryEvent) {
        Sentry.captureEvent(event)
    }
}

internal fun interface SentryLogCapture {
    fun info(message: String, attributes: Map<String, Any>)
}

internal class RealSentryLogCapture @Inject constructor() : SentryLogCapture {
    override fun info(message: String, attributes: Map<String, Any>) {
        // named() keeps each value's type: a Long is sent as an integer, so it can be summed and charted.
        val sentryAttributes = SentryAttributes.of(*attributes.map { (key, value) -> SentryAttribute.named(key, value) }.toTypedArray())
        Sentry.logger().log(SentryLogLevel.INFO, SentryLogParameters.create(sentryAttributes), message)
    }
}

/**
 * A failure is a Sentry event, so it raises an issue. Everything else is a measurement, not a problem, and goes
 * to Sentry Logs: as an event, each name became an error issue of its own that nobody could act on.
 */
internal class RealHostEventCapture @Inject constructor(
    private val enabledProvider: HostTelemetryEnabledProvider,
    private val sentryCapture: SentryEventCapture,
    private val logCapture: SentryLogCapture,
) : HostEventCapture {
    override fun capture(event: () -> HostEvent) {
        if (!enabledProvider.isEnabled()) return
        val hostEvent = event()
        if (hostEvent.isFailure) {
            sentryCapture.capture(hostEvent.toSentryEvent())
        } else {
            logCapture.info(hostEvent.name, hostEvent.toLogAttributes())
        }
    }
}

internal class RealHostMonotonicTimeProvider @Inject constructor() : HostMonotonicTimeProvider {
    override fun nowMillis(): Long = SystemClock.elapsedRealtime()
}

internal class RealHostEventOriginProvider @Inject constructor() : HostEventOriginProvider {
    override fun current() = HostEventOrigin(
        appVariant = BuildConfig.BUILD_TYPE,
        appFlavor = BuildConfig.FLAVOR,
        nativeAppVersion = BuildConfig.VERSION_NAME,
        platform = "android",
        osVersion = Build.VERSION.RELEASE,
        appBuild = BuildConfig.VERSION_CODE.toLong(),
    )
}

internal class HostProcessTelemetryInitializer @Inject constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        capture.capture {
            originProvider.current().processStarted(
                launchId = UUID.randomUUID(),
                monotonicMs = timeProvider.nowMillis(),
            )
        }
    }
}

private fun HostEventOrigin.processStarted(launchId: UUID, monotonicMs: Long) = HostEvent(
    name = "host.process.started",
    tags = baseTags() + ("host.os_version" to osVersion),
    extras = baseExtras() + mapOf(
        "app.launch_id" to launchId.toString(),
        "event.monotonic_ms" to monotonicMs,
    ),
    isFailure = false,
)

/**
 * A log has one set of attributes where an event has tags and extras. A tag keeps its value: an extra under the
 * same name would otherwise overwrite it. `tag` is the attribute the host's other Sentry logs are filtered by.
 */
internal fun HostEvent.toLogAttributes(): Map<String, Any> =
    extras + tags + mapOf(LOG_TAG_ATTRIBUTE to LOG_TAG, EVENT_NAME_ATTRIBUTE to name)

private const val LOG_TAG = "host.telemetry"
private const val LOG_TAG_ATTRIBUTE = "tag"
private const val EVENT_NAME_ATTRIBUTE = "event.name"

private fun HostEvent.toSentryEvent() = SentryEvent().apply {
    message = Message().apply { formatted = name }
    // One group per event name. Sentry would otherwise group these by the stack
    // trace it attaches, which splits one name across several groups.
    fingerprints = listOf(this@toSentryEvent.name)
    level = if (isFailure) SentryLevel.ERROR else SentryLevel.INFO
    logger = LOG_TAG
    this@toSentryEvent.tags.forEach(::setTag)
    this@toSentryEvent.extras.forEach(::setExtra)
}
