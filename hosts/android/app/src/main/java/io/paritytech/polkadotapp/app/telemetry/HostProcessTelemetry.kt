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
import io.sentry.SentryEvent
import io.sentry.SentryLevel
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
) : HostEventCapture {
    override fun capture(event: () -> HostEvent) {
        if (enabledProvider.isEnabled()) {
            sentryCapture.capture(event().toSentryEvent())
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
)

private fun HostEvent.toSentryEvent() = SentryEvent().apply {
    message = Message().apply { formatted = name }
    level = SentryLevel.INFO
    logger = "host.telemetry"
    this@toSentryEvent.tags.forEach(::setTag)
    this@toSentryEvent.extras.forEach(::setExtra)
}
