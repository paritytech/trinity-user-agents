package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.os.SystemClock
import dagger.hilt.android.qualifiers.ApplicationContext
import io.sentry.Sentry
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import io.sentry.protocol.Message
import timber.log.Timber
import uniffi.truapi.truapiHostVersion
import java.util.UUID
import javax.inject.Inject

internal data class HostWebViewCreatedEvent(
    val sessionId: String,
    val appVariant: String,
    val nativeAppVersion: String,
    val hostVersion: String,
    val appBuild: Long,
    val monotonicMs: Long,
)

class HostWebViewCreatedTelemetry private constructor(
    private val packageName: String,
    private val metadata: () -> HostWebViewCreatedEvent,
    private val capture: (SentryEvent) -> Unit,
) {
    @Inject
    constructor(@ApplicationContext context: Context) : this(
        packageName = context.packageName,
        metadata = {
            val packageInfo = context.packageManager.getPackageInfo(context.packageName, 0)
            HostWebViewCreatedEvent(
                sessionId = "",
                appVariant = appVariant(context.packageName),
                nativeAppVersion = packageInfo.versionName.orEmpty(),
                hostVersion = truapiHostVersion(),
                appBuild = packageInfo.longVersionCode,
                monotonicMs = SystemClock.elapsedRealtime(),
            )
        },
        capture = { Sentry.captureEvent(it) },
    )

    fun report(sessionId: String) {
        if (!shouldInstallHostTelemetry(packageName)) return

        runCatching { capture(metadata().copy(sessionId = sessionId).toSentryEvent()) }
            .onFailure { Timber.w(it, "Failed to capture native host WebView event") }
    }

    companion object {
        const val EVENT_NAME = "host.webview.created"

        internal fun createForTest(
            packageName: String,
            metadata: () -> HostWebViewCreatedEvent,
            capture: (SentryEvent) -> Unit,
        ) = HostWebViewCreatedTelemetry(packageName, metadata, capture)

        internal fun newSessionId(): String = UUID.randomUUID().toString()

        private fun appVariant(packageName: String): String = when {
            packageName.endsWith(".nightly") -> "nightly"
            packageName.endsWith(".safetynet") -> "safetynet"
            packageName.endsWith(".debug") -> "debug"
            else -> "release"
        }
    }
}

private fun HostWebViewCreatedEvent.toSentryEvent() = SentryEvent().apply {
    message = Message().apply { formatted = HostWebViewCreatedTelemetry.EVENT_NAME }
    level = SentryLevel.INFO
    setTag("host.session_id", sessionId)
    setTag("host.app_variant", appVariant)
    setTag("host.native_app_version", nativeAppVersion)
    setTag("host.platform", "android")
    setTag("truapi.host_version", hostVersion)
    setExtra("host.app_build", appBuild)
    setExtra("event.monotonic_ms", monotonicMs)
}
