package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostPageOpen
import io.paritytech.polkadotapp.common.data.telemetry.HostPageSession
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
    /** The session's open is the tap or link that asked for the page, if any: its source is tagged and its age reported. */
    fun report(session: HostPageSession) {
        runCatching {
            capture.capture {
                originProvider.current().webViewCreated(
                    session = session,
                    hostVersion = truapiHostVersion(),
                    monotonicMs = timeProvider.nowMillis(),
                )
            }
        }.logFailure("Failed to capture native host WebView event")
    }

    /** Read when the WebView is created, to time [reportVisible] from it. */
    fun webViewCreatedAtMs(): Long = timeProvider.nowMillis()

    /** The page's first visible commit, timed from the tap or link that asked for it, if any. */
    fun reportVisible(session: HostPageSession, webViewCreatedAtMs: Long) {
        runCatching {
            capture.capture {
                val nowMs = timeProvider.nowMillis()
                originProvider.current().pageVisible(
                    session = session,
                    webViewMs = nowMs - webViewCreatedAtMs,
                    hostVersion = truapiHostVersion(),
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
    session: HostPageSession,
    hostVersion: String,
    monotonicMs: Long,
) = HostEvent(
    name = HostWebViewCreatedTelemetry.EVENT_NAME,
    tags = baseTags() + ("truapi.host_version" to hostVersion) + session.tags(),
    extras = baseExtras() + session.extras(monotonicMs),
    isFailure = false,
)

private fun HostEventOrigin.pageVisible(
    session: HostPageSession,
    webViewMs: Long,
    hostVersion: String,
    monotonicMs: Long,
) = HostEvent(
    name = HostWebViewCreatedTelemetry.VISIBLE_EVENT_NAME,
    tags = baseTags() + ("truapi.host_version" to hostVersion) + session.tags(),
    extras = baseExtras() + session.extras(monotonicMs) + ("webview_ms" to webViewMs),
    isFailure = false,
)

private fun HostPageSession.tags(): Map<String, String> = mapOf("host.session_id" to id) +
    listOfNotNull(open?.takeIf { loadAttempt == 1 }?.let { "open.source" to it.source.tag })

private fun HostPageSession.extras(monotonicMs: Long): Map<String, Any> = mapOf(
    "event.monotonic_ms" to monotonicMs,
    "load.attempt" to loadAttempt,
) + listOfNotNull(open?.takeIf { loadAttempt == 1 }?.tapMs(monotonicMs))

private fun HostPageOpen.tapMs(monotonicMs: Long) = "tap_ms" to (monotonicMs - this.monotonicMs)
