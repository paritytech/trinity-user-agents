package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostPageOpen
import io.paritytech.polkadotapp.common.data.telemetry.baseExtras
import io.paritytech.polkadotapp.common.data.telemetry.baseTags
import io.paritytech.polkadotapp.common.utils.logFailure
import uniffi.truapi.truapiHostVersion
import javax.inject.Inject

class HostWebViewCreatedTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) {
    /** [open] is the tap or link that asked for the page, if any: its source is tagged and its age reported. */
    fun report(sessionId: String, open: HostPageOpen? = null, nowEpochMs: Long = System.currentTimeMillis()) {
        runCatching {
            capture.capture {
                originProvider.current().webViewCreated(
                    sessionId = sessionId,
                    hostVersion = truapiHostVersion(),
                    monotonicMs = timeProvider.nowMillis(),
                    open = open,
                    nowEpochMs = nowEpochMs,
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
    open: HostPageOpen?,
    nowEpochMs: Long,
) = HostEvent(
    name = HostWebViewCreatedTelemetry.EVENT_NAME,
    tags = baseTags() + mapOf("truapi.host_version" to hostVersion, "host.session_id" to sessionId) +
        listOfNotNull(open?.let { "open.source" to it.source.tag }),
    extras = baseExtras() + mapOf("event.monotonic_ms" to monotonicMs) +
        listOfNotNull(open?.let { "tap_ms" to nowEpochMs - it.epochMs }),
)
