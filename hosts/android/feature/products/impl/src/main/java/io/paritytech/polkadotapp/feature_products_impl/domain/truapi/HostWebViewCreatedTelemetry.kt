package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.os.SystemClock
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_products_impl.BuildConfig
import io.sentry.Sentry
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import io.sentry.protocol.Message
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import java.util.UUID
import javax.inject.Inject
import uniffi.truapi.truapiHostVersion

internal data class HostWebViewCreatedEvent(
    val sessionId: String,
    val appVariant: String,
    val nativeAppVersion: String,
    val hostVersion: String,
    val appBuild: Long,
    val monotonicMs: Long,
)

class HostWebViewCreatedTelemetry private constructor(
    private val enabled: () -> Boolean,
    private val metadata: () -> HostWebViewCreatedEvent,
    private val capture: (SentryEvent) -> Unit,
) {
    @Inject
    constructor(@ApplicationContext context: Context) : this(
        enabled = { FeatureOption.HOST_TELEMETRY.isEnabled },
        metadata = {
            val packageInfo = context.packageManager.getPackageInfo(context.packageName, 0)
            HostWebViewCreatedEvent(
                sessionId = "",
                appVariant = appVariant(BuildConfig.BUILD_TYPE),
                nativeAppVersion = packageInfo.versionName.orEmpty(),
                hostVersion = truapiHostVersion(),
                appBuild = packageInfo.longVersionCode,
                monotonicMs = SystemClock.elapsedRealtime(),
            )
        },
        capture = { Sentry.captureEvent(it) },
    )

    fun report(sessionId: String) {
        if (!enabled()) return

        runCatching { capture(metadata().copy(sessionId = sessionId).toSentryEvent()) }
            .logFailure("Failed to capture native host WebView event")
    }

    companion object {
        const val EVENT_NAME = "host.webview.created"

        internal fun createForTest(
            enabled: () -> Boolean,
            metadata: () -> HostWebViewCreatedEvent,
            capture: (SentryEvent) -> Unit,
        ) = HostWebViewCreatedTelemetry(enabled, metadata, capture)

        internal fun newSessionId(): String = UUID.randomUUID().toString()

        internal fun appVariant(buildType: String): String = buildType
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
