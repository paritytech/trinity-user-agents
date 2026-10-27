package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.utils.logFailure
import uniffi.truapi.truapiHostVersion
import javax.inject.Inject

class HostWebViewCreatedTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) {
    fun report(sessionId: String) {
        runCatching {
            capture.capture {
                originProvider.current().webViewCreated(
                    sessionId = sessionId,
                    hostVersion = truapiHostVersion(),
                    monotonicMs = timeProvider.nowMillis(),
                )
            }
        }.logFailure("Failed to capture native host WebView event")
    }

    companion object {
        const val EVENT_NAME = "host.webview.created"
    }
}

private fun HostEventOrigin.webViewCreated(
    sessionId: String,
    hostVersion: String,
    monotonicMs: Long,
) = HostEvent(
    name = HostWebViewCreatedTelemetry.EVENT_NAME,
    tags = mapOf(
        "host.app_variant" to appVariant,
        "host.app_flavor" to appFlavor,
        "host.native_app_version" to nativeAppVersion,
        "host.platform" to platform,
        "truapi.host_version" to hostVersion,
    ),
    extras = mapOf(
        "host.session_id" to sessionId,
        "host.app_build" to appBuild,
        "event.monotonic_ms" to monotonicMs,
    ),
)
