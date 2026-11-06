package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostPocketTaps
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.PageReveal
import uniffi.truapi.ProductId
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject

/** A product page's load and reveal, each timed from when its open was asked for. */
class HostPageOpenTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
    private val taps: HostPocketTaps,
) {
    /** One open, asked for now. Each of its events is reported at most once. */
    fun open(): PageOpen = PageOpen(timeProvider.nowMillis())

    inner class PageOpen internal constructor(private val requestedAtMillis: Long) {
        @Volatile private var productId: ProductId? = null

        @Volatile private var sessionId: String? = null

        @Volatile private var tappedAtMillis: Long? = null
        private val loadReported = AtomicBoolean(false)
        private val revealReported = AtomicBoolean(false)

        fun opened(productId: ProductId, sessionId: String) {
            this.productId = productId
            this.sessionId = sessionId
            tappedAtMillis = taps.consume(productId.value)
        }

        /** A page that is not a product's (the debug browser opening any URL) has no load to report. */
        fun notAProduct() {
            loadReported.set(true)
            revealReported.set(true)
        }

        fun finished(loaded: Boolean) = reportLoad(if (loaded) HostEventOutcome.OK else HostEventOutcome.ERROR)

        /** The open failed before the page was asked to load, so no finish will ever arrive. */
        fun failedToStart() = reportLoad(HostEventOutcome.ERROR)

        /** The page went away; only a load still outstanding is reported, as abandoned. */
        fun closed() = reportLoad(HostEventOutcome.ABANDONED)

        fun revealed(via: PageReveal) {
            if (!revealReported.compareAndSet(false, true)) return
            report(REVEALED_EVENT_NAME, HostEventOutcome.OK, mapOf("via" to via.tag)) { emptyMap() }
        }

        private fun reportLoad(outcome: HostEventOutcome) {
            if (!loadReported.compareAndSet(false, true)) return
            report(PAGE_LOADED_EVENT_NAME, outcome, emptyMap()) { now ->
                tappedAtMillis?.let { mapOf("tap_ms" to now - it) }.orEmpty()
            }
        }

        private fun report(
            name: String,
            outcome: HostEventOutcome,
            tags: Map<String, String>,
            extras: (now: Long) -> Map<String, Any>,
        ) {
            runCatching {
                capture.capture {
                    val now = timeProvider.nowMillis()
                    originProvider.current().stepEvent(
                        name = name,
                        outcome = outcome,
                        monotonicMs = now,
                        durationMs = now - requestedAtMillis,
                        sessionId = sessionId,
                        tags = productId?.let { mapOf("product.id" to it.value) }.orEmpty() + tags,
                        extras = extras(now),
                    )
                }
            }.logFailure("Failed to capture native host $name event")
        }
    }

    companion object {
        const val PAGE_LOADED_EVENT_NAME = "host.webview.page_loaded"
        const val REVEALED_EVENT_NAME = "host.webview.revealed"
    }
}
