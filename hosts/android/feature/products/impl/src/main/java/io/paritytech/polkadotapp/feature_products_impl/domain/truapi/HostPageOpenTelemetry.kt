package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostCacheHits
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenMark
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenMarks
import io.paritytech.polkadotapp.common.data.telemetry.HostSessionIds
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.PageReveal
import uniffi.truapi.ProductId
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject

/**
 * A product page's load and reveal, each timed from when its open was asked for, and the Pocket card
 * tap that opened it or showed it again.
 */
class HostPageOpenTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
    private val marks: HostOpenMarks,
    private val sessionIds: HostSessionIds,
    private val cacheHits: HostCacheHits,
) {
    /** One open, asked for now. Each of its events is reported at most once. */
    fun open(): PageOpen = PageOpen(timeProvider.nowMillis())

    inner class PageOpen internal constructor(private val requestedAtMillis: Long) {
        @Volatile private var productId: ProductId? = null

        @Volatile private var sessionId: String? = null

        @Volatile private var tap: HostOpenMark? = null
        private val loadReported = AtomicBoolean(false)
        private val revealReported = AtomicBoolean(false)

        /** Handed to the page, so its own timing starts at the tap that opened it. Null for any other open. */
        val pageOrigin: HostPageOrigin?
            get() = tap?.let { HostPageOrigin(it.atEpochMillis, it.source.tag) }

        /**
         * Takes the open asked for [productId] and returns the page's host session: the one minted with
         * the tap or link, which the work started for it already reports under, or a new one.
         */
        fun opened(productId: ProductId): String {
            this.productId = productId
            val mark = marks.consume(productId.value)
            val session = mark?.sessionId ?: sessionIds.next()
            sessionId = session
            tap = mark?.also { reportTap(it, HostEventOutcome.OPENED) }
            return session
        }

        /** The tap that shows this page again, held and loaded already; null when no tap asked for it. */
        fun reuseTap(): HostOpenMark? = productId?.let { marks.consume(it.value) }

        /** Reports [tap] as having shown this page again, now that the page is visible. */
        fun reused(tap: HostOpenMark) = reportTap(tap, HostEventOutcome.REUSED)

        /** A page that is not a product's (the debug browser opening any URL) has no load to report. */
        fun notAProduct() {
            loadReported.set(true)
            revealReported.set(true)
        }

        fun finished(loaded: Boolean) = reportLoad(if (loaded) HostEventOutcome.OK else HostEventOutcome.ERROR)

        /** The open failed before the page was asked to load, so no finish will ever arrive. */
        fun failedToStart() = reportLoad(HostEventOutcome.ERROR)

        /** The page went away; only a load still outstanding is reported, as abandoned. */
        fun closed() {
            reportLoad(HostEventOutcome.ABANDONED)
            sessionId?.let(cacheHits::forget)
        }

        fun revealed(via: PageReveal) {
            if (!revealReported.compareAndSet(false, true)) return
            report(REVEALED_EVENT_NAME, HostEventOutcome.OK, mapOf("reveal.via" to via.tag)) { emptyMap() }
        }

        private fun reportLoad(outcome: HostEventOutcome) {
            if (!loadReported.compareAndSet(false, true)) return
            report(PAGE_LOADED_EVENT_NAME, outcome, emptyMap()) { now ->
                tap?.let { mapOf("tap_ms" to now - it.atMillis) }.orEmpty()
            }
        }

        // Timed from the tap or link: to the load starting when it opened the page, to the page being
        // visible when it showed a held one again.
        private fun reportTap(tap: HostOpenMark, outcome: HostEventOutcome) = report(
            CARD_TAPPED_EVENT_NAME,
            outcome,
            mapOf("open.source" to tap.source.tag),
            startedAtMillis = tap.atMillis,
        ) { now ->
            if (outcome == HostEventOutcome.REUSED) mapOf("reuse_ms" to now - tap.atMillis) else emptyMap()
        }

        private fun report(
            name: String,
            outcome: HostEventOutcome,
            tags: Map<String, String>,
            startedAtMillis: Long = requestedAtMillis,
            extras: (now: Long) -> Map<String, Any>,
        ) {
            runCatching {
                capture.capture {
                    val now = timeProvider.nowMillis()
                    originProvider.current().stepEvent(
                        name = name,
                        outcome = outcome,
                        monotonicMs = now,
                        durationMs = now - startedAtMillis,
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
        const val CARD_TAPPED_EVENT_NAME = "host.pocket.card_tapped"
    }
}
