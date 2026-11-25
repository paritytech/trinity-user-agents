package io.paritytech.polkadotapp.app.telemetry

import android.os.Build
import android.os.SystemClock
import android.util.Log
import io.paritytech.polkadotapp.app.BuildConfig
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventLevel
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostLogSink
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.baseExtras
import io.paritytech.polkadotapp.common.data.telemetry.baseTags
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.sentry.Sentry
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import io.sentry.protocol.Message
import java.util.UUID
import javax.inject.Inject
import javax.inject.Provider

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
    // A Provider, so the sink reads the Sentry logger once Sentry has started rather than when this is built.
    private val logSink: Provider<HostLogSink>,
) : HostEventCapture {
    override fun capture(event: () -> HostEvent) {
        if (enabledProvider.isEnabled()) {
            val hostEvent = event()
            when (hostEvent.level) {
                HostEventLevel.INFO ->
                    logSink.get().log(Log.INFO, HOST_TELEMETRY_LOGGER, hostEvent.name, hostEvent.toLogAttributes())
                HostEventLevel.WARNING, HostEventLevel.ERROR -> sentryCapture.capture(hostEvent.toSentryEvent())
            }
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
    level = HostEventLevel.INFO,
    tags = baseTags() + ("host.os_version" to osVersion),
    extras = baseExtras() + mapOf(
        "app.launch_id" to launchId.toString(),
        "event.monotonic_ms" to monotonicMs,
    ),
)

private fun HostEvent.toSentryEvent() = SentryEvent().apply {
    message = Message().apply { formatted = name }
    // One group per event name. Sentry would otherwise group these by the stack
    // trace it attaches, which splits one name across several groups.
    fingerprints = listOf(this@toSentryEvent.name)
    level = this@toSentryEvent.level.toSentryLevel()
    logger = HOST_TELEMETRY_LOGGER
    this@toSentryEvent.tags.forEach(::setTag)
    this@toSentryEvent.extras.forEach(::setExtra)
}

private fun HostEventLevel.toSentryLevel() = when (this) {
    HostEventLevel.INFO -> SentryLevel.INFO
    HostEventLevel.WARNING -> SentryLevel.WARNING
    HostEventLevel.ERROR -> SentryLevel.ERROR
}

// Tags and extras share one attribute namespace on a log line, so an extra wins
// over a tag of the same key. The sink adds `tag` itself.
private fun HostEvent.toLogAttributes(): Map<String, Any> = tags + extras + ("event.name" to name)

private const val HOST_TELEMETRY_LOGGER = "host.telemetry"
