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
    fun report(sessionId: String, open: HostPageOpen? = null) {
        runCatching {
            capture.capture {
                originProvider.current().webViewCreated(
                    sessionId = sessionId,
                    hostVersion = truapiHostVersion(),
                    monotonicMs = timeProvider.nowMillis(),
                    open = open,
                )
            }
        }.logFailure("Failed to capture native host WebView event")
    }

    /** Read when the WebView is created, to time [reportVisible] from it. */
    fun webViewCreatedAtMs(): Long = timeProvider.nowMillis()

    /** The page's first visible commit; [open] is the tap or link that asked for it, if any. */
    fun reportVisible(
        sessionId: String,
        webViewCreatedAtMs: Long,
        open: HostPageOpen? = null,
    ) {
        runCatching {
            capture.capture {
                val nowMs = timeProvider.nowMillis()
                originProvider.current().pageVisible(
                    sessionId = sessionId,
                    webViewMs = nowMs - webViewCreatedAtMs,
                    open = open,
                    monotonicMs = nowMs,
                )
            }
        }.logFailure("Failed to capture native host page visible event")
    }

    companion object {
        const val EVENT_NAME = "host.webview.created"
        const val VISIBLE_EVENT_NAME = "host.page.visible"
    }
}

private fun HostEventOrigin.webViewCreated(
    sessionId: String,
    hostVersion: String,
    monotonicMs: Long,
    open: HostPageOpen?,
) = HostEvent(
    name = HostWebViewCreatedTelemetry.EVENT_NAME,
    tags = baseTags() + mapOf("truapi.host_version" to hostVersion, "host.session_id" to sessionId) +
        listOfNotNull(open?.let { "open.source" to it.source.tag }),
    extras = baseExtras() + mapOf("event.monotonic_ms" to monotonicMs) +
        listOfNotNull(open?.tapMs(monotonicMs)),
    isFailure = false,
)

private fun HostEventOrigin.pageVisible(
    sessionId: String,
    webViewMs: Long,
    open: HostPageOpen?,
    monotonicMs: Long,
) = HostEvent(
    name = HostWebViewCreatedTelemetry.VISIBLE_EVENT_NAME,
    tags = baseTags() + mapOf("host.session_id" to sessionId) +
        listOfNotNull(open?.let { "open.source" to it.source.tag }),
    extras = baseExtras() + mapOf("webview_ms" to webViewMs) +
        listOfNotNull(open?.tapMs(monotonicMs)),
    isFailure = false,
)

// Both readings are the monotonic clock, so a wall clock change cannot move the gap. A negative gap would be
// a bug, and is left out rather than sent as a clamped zero that reads as the fastest possible open.
private fun HostPageOpen.tapMs(monotonicMs: Long) =
    (monotonicMs - this.monotonicMs).takeIf { it >= 0 }?.let { "tap_ms" to it }
