package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.os.SystemClock
import dagger.hilt.android.qualifiers.ApplicationContext
import io.sentry.Sentry
import io.sentry.SentryLevel
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

internal class HostWebViewCreatedTelemetry private constructor(
    private val packageName: String,
    private val metadata: () -> HostWebViewCreatedEvent,
    private val capture: (HostWebViewCreatedEvent) -> Unit,
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
        capture = ::captureHostWebViewCreatedInSentry,
    )

    fun report(sessionId: String) {
        if (!shouldInstallHostTelemetry(packageName)) return

        runCatching { capture(metadata().copy(sessionId = sessionId)) }
            .onFailure { Timber.w(it, "Failed to capture native host WebView event") }
    }

    companion object {
        const val EVENT_NAME = "host.webview.created"

        internal fun createForTest(
            packageName: String,
            metadata: () -> HostWebViewCreatedEvent,
            capture: (HostWebViewCreatedEvent) -> Unit,
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

private fun captureHostWebViewCreatedInSentry(event: HostWebViewCreatedEvent) {
    Sentry.captureMessage(HostWebViewCreatedTelemetry.EVENT_NAME, SentryLevel.INFO) { scope ->
        scope.setTag("host.session_id", event.sessionId)
        scope.setTag("host.app_variant", event.appVariant)
        scope.setTag("host.native_app_version", event.nativeAppVersion)
        scope.setTag("host.platform", "android")
        scope.setTag("truapi.host_version", event.hostVersion)
        scope.setExtra("host.app_build", event.appBuild)
        scope.setExtra("event.monotonic_ms", event.monotonicMs)
    }
}
