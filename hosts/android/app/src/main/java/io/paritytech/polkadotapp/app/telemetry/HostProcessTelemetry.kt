package io.paritytech.polkadotapp.app.telemetry

import android.os.Build
import android.os.SystemClock
import io.paritytech.polkadotapp.app.BuildConfig
import io.paritytech.polkadotapp.app.monitoring.SentryMetricsCapture
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostSessionIds
import io.paritytech.polkadotapp.common.data.telemetry.HostWallClock
import io.paritytech.polkadotapp.common.data.telemetry.baseExtras
import io.paritytech.polkadotapp.common.data.telemetry.baseTags
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.sentry.Sentry
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import io.sentry.metrics.MetricsUnit.Duration.MILLISECOND
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

internal class RealHostEventCapture @Inject constructor(
    private val enabledProvider: HostTelemetryEnabledProvider,
    private val sentryCapture: SentryEventCapture,
    private val metrics: SentryMetricsCapture,
) : HostEventCapture {
    override val enabled: Boolean get() = enabledProvider.isEnabled()

    override fun capture(event: () -> HostEvent) {
        if (enabledProvider.isEnabled()) {
            val hostEvent = event()
            sentryCapture.capture(hostEvent.toSentryEvent())
            hostEvent.durationMs?.let { durationMs ->
                metrics.distribution(hostEvent.name, durationMs.toDouble(), MILLISECOND, hostEvent.chartAttributes())
            }
        }
    }
}

internal class RealHostMonotonicTimeProvider @Inject constructor() : HostMonotonicTimeProvider {
    override fun nowMillis(): Long = SystemClock.elapsedRealtime()
}

internal class RealHostWallClock @Inject constructor() : HostWallClock {
    override fun nowEpochMillis(): Long = System.currentTimeMillis()
}

internal class RealHostSessionIds @Inject constructor() : HostSessionIds {
    override fun next(): String = UUID.randomUUID().toString()
}

internal class RealHostEventOriginProvider @Inject constructor() : HostEventOriginProvider {
    override fun current() = HostEventOrigin(
        appVariant = BuildConfig.BUILD_TYPE,
        appFlavor = BuildConfig.FLAVOR,
        nativeAppVersion = BuildConfig.VERSION_NAME,
        platform = "android",
        osVersion = Build.VERSION.RELEASE,
        appBuild = BuildConfig.VERSION_CODE.toLong(),
        launchId = LAUNCH_ID,
    )

    private companion object {
        // One per process, so every event of a launch joins its host.process.started.
        val LAUNCH_ID = UUID.randomUUID().toString()
    }
}

internal class HostProcessTelemetryInitializer @Inject constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        capture.capture {
            originProvider.current().processStarted(monotonicMs = timeProvider.nowMillis())
        }
    }
}

private fun HostEventOrigin.processStarted(monotonicMs: Long) = HostEvent(
    name = "host.process.started",
    tags = baseTags() + ("host.os_version" to osVersion),
    extras = baseExtras() + mapOf(
        "app.launch_id" to launchId,
        "event.monotonic_ms" to monotonicMs,
    ),
)

// A distribution's attributes are aggregated, so the per-open session stays on the event alone.
private fun HostEvent.chartAttributes(): Map<String, String> = tags - "host.session_id"

private fun HostEvent.toSentryEvent() = SentryEvent().apply {
    message = Message().apply { formatted = name }
    level = SentryLevel.INFO
    logger = "host.telemetry"
    this@toSentryEvent.tags.forEach(::setTag)
    this@toSentryEvent.extras.forEach(::setExtra)
}
