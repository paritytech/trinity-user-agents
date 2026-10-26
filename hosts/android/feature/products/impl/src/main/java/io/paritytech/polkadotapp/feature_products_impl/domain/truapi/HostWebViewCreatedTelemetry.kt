package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.os.SystemClock
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.BuildConfig
import io.sentry.Sentry
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import io.sentry.protocol.Message
import uniffi.truapi.truapiHostVersion
import javax.inject.Inject

internal data class HostWebViewCreatedEvent(
    val sessionId: String,
    val appVariant: String,
    val nativeAppVersion: String,
    val hostVersion: String,
    val appBuild: Long,
    val monotonicMs: Long,
)

internal fun interface HostWebViewCreatedEventCapture {
    fun capture(event: () -> SentryEvent)
}

internal class SentryHostWebViewCreatedEventCapture @Inject constructor() : HostWebViewCreatedEventCapture {
    override fun capture(event: () -> SentryEvent) {
        if (FeatureOption.HOST_TELEMETRY.isEnabled) {
            Sentry.captureEvent(event())
        }
    }
}

internal fun interface HostMonotonicTimeProvider {
    fun nowMillis(): Long
}

internal class AndroidHostMonotonicTimeProvider @Inject constructor() : HostMonotonicTimeProvider {
    override fun nowMillis(): Long = SystemClock.elapsedRealtime()
}

class HostWebViewCreatedTelemetry @Inject internal constructor(
    @param:ApplicationContext private val context: Context,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostWebViewCreatedEventCapture,
) {
    fun report(sessionId: String) {
        runCatching { capture.capture { metadata(sessionId).toSentryEvent() } }
            .logFailure("Failed to capture native host WebView event")
    }

    private fun metadata(sessionId: String): HostWebViewCreatedEvent {
        val packageInfo = context.packageManager.getPackageInfo(context.packageName, 0)
        return HostWebViewCreatedEvent(
            sessionId = sessionId,
            appVariant = BuildConfig.BUILD_TYPE,
            nativeAppVersion = packageInfo.versionName.orEmpty(),
            hostVersion = truapiHostVersion(),
            appBuild = packageInfo.longVersionCode,
            monotonicMs = timeProvider.nowMillis(),
        )
    }

    companion object {
        const val EVENT_NAME = "host.webview.created"
    }
}

private fun HostWebViewCreatedEvent.toSentryEvent() = SentryEvent().apply {
    message = Message().apply { formatted = HostWebViewCreatedTelemetry.EVENT_NAME }
    level = SentryLevel.INFO
    logger = "host.telemetry"
    setExtra("host.session_id", sessionId)
    setTag("host.app_variant", appVariant)
    setTag("host.native_app_version", nativeAppVersion)
    setTag("host.platform", "android")
    setTag("truapi.host_version", hostVersion)
    setExtra("host.app_build", appBuild)
    setExtra("event.monotonic_ms", monotonicMs)
}
