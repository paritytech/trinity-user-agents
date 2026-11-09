package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.telemetry.HostCacheHits
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadTrigger
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenMarks
import io.paritytech.polkadotapp.common.data.telemetry.HostSessionIds
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.PageReveal
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductId

class HostPageOpenTelemetryTest {
    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private var now = TAPPED_AT
    private val timeProvider = HostMonotonicTimeProvider { now }
    private val captured = mutableListOf<HostEvent>()
    private val capture = HostEventCapture { captured += it() }
    private var minted = 0
    private val sessionIds = HostSessionIds { "session-${++minted}" }
    private val cacheHits = HostCacheHits()
    private val taps = HostOpenMarks(timeProvider, { TAPPED_AT_EPOCH }, capture, sessionIds, cacheHits)
    private val telemetry = HostPageOpenTelemetry(originProvider, timeProvider, capture, taps, sessionIds, cacheHits)

    @Test
    fun `reports the page load under its session, timed from the open and from the card tap`() {
        taps.tapped("humanity.dot")
        now = OPEN_REQUESTED_AT
        val page = telemetry.open()
        page.opened(HUMANITY)
        now = LOADED_AT

        page.finished(loaded = true)

        assertEquals(
            HostEvent(
                name = "host.webview.page_loaded",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "nightly",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "outcome" to "ok",
                    "host.os_version" to "15",
                    "host.session_id" to "session-1",
                    "product.id" to "humanity.dot",
                ),
                extras = mapOf(
                    "host.app_build" to 123L,
                    "event.monotonic_ms" to LOADED_AT,
                    "duration_ms" to LOADED_AT - OPEN_REQUESTED_AT,
                    "app.launch_id" to "launch-7",
                    "host.session_id" to "session-1",
                    "tap_ms" to LOADED_AT - TAPPED_AT,
                ),
                durationMs = LOADED_AT - OPEN_REQUESTED_AT,
            ),
            captured.single { it.name == "host.webview.page_loaded" },
        )
    }

    // The tap's own fetch already reports under this session, so the page's lookups join it.
    @Test
    fun `a page takes the session its tap minted, and one nothing asked for mints its own`() {
        taps.tapped("humanity.dot")
        val tapSession = taps.pendingSession("humanity.dot")

        val tapped = telemetry.open().opened(HUMANITY)
        val untapped = telemetry.open().opened(HUMANITY)

        assertEquals(listOf("session-1", "session-1", "session-2"), listOf(tapSession, tapped, untapped))
    }

    // Each page claims one report per step in its session; claims outliving the page would only grow.
    @Test
    fun `closing a page forgets what its session reported`() {
        val page = telemetry.open()
        val load = HostLoadContext(HostLoadTrigger.OPEN, page.opened(HUMANITY))
        cacheHits.isFirst("host.product.resolved", "humanity.dot", load)

        page.closed()

        assertTrue(cacheHits.isFirst("host.product.resolved", "humanity.dot", load))
    }

    @Test
    fun `the tap that opens a page is reported as opened, and handed to the page`() {
        taps.tapped("humanity.dot")
        now = OPEN_REQUESTED_AT
        val page = telemetry.open()

        page.opened(HUMANITY)

        val tapped = captured.single()
        assertEquals("host.pocket.card_tapped" to "opened", tapped.name to tapped.tags["outcome"])
        assertEquals("pocket_card", tapped.tags["open.source"])
        assertEquals("session-1" to "humanity.dot", tapped.tags["host.session_id"] to tapped.tags["product.id"])
        assertEquals(OPEN_REQUESTED_AT - TAPPED_AT, tapped.durationMs)
        assertEquals(TAPPED_AT_EPOCH to "pocket_card", page.pageOrigin?.openedAtEpochMs to page.pageOrigin?.openSource)
    }

    @Test
    fun `a page a link opened is handed the link's time as a deeplink open`() {
        val link = taps.linkHandled()
        taps.linked("humanity.dot", link)
        val page = telemetry.open()

        page.opened(HUMANITY)

        assertEquals(TAPPED_AT_EPOCH to "deeplink", page.pageOrigin?.openedAtEpochMs to page.pageOrigin?.openSource)
        val tapped = captured.single()
        assertEquals("opened" to "deeplink", tapped.tags["outcome"] to tapped.tags["open.source"])
    }

    // A held page shown again starts no load, so without this the tap would go unreported.
    @Test
    fun `a tap that shows a held page again is reported as reused, timed to the page being visible`() {
        val page = telemetry.open().apply { opened(HUMANITY) }
        now = 5_000L
        taps.tapped("humanity.dot")

        val tap = checkNotNull(page.reuseTap())
        now = 5_120L
        page.reused(tap)

        val reused = captured.single()
        assertEquals("host.pocket.card_tapped" to "reused", reused.name to reused.tags["outcome"])
        assertEquals(120L, reused.extras["reuse_ms"])
        assertEquals("session-1", reused.tags["host.session_id"])
    }

    @Test
    fun `a page opened without a card tap carries no tap time`() {
        val page = telemetry.open()
        page.opened(HUMANITY)

        page.finished(loaded = true)

        assertFalse(captured.single().extras.containsKey("tap_ms"))
    }

    @Test
    fun `a page whose main document failed is reported as an error`() {
        val page = telemetry.open().apply { opened(HUMANITY) }

        page.finished(loaded = false)

        assertEquals(listOf("error"), captured.map { it.tags["outcome"] })
    }

    // Without it, a page the user gave up on is missing from the data rather than counted as slow.
    @Test
    fun `a page closed before it finished loading is reported as abandoned, and only that`() {
        val page = telemetry.open().apply { opened(HUMANITY) }

        page.closed()
        page.finished(loaded = true)

        assertEquals(listOf("abandoned"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `closing a page that already loaded reports nothing more`() {
        val page = telemetry.open().apply { opened(HUMANITY) }

        page.finished(loaded = true)
        page.closed()

        assertEquals(listOf("ok"), captured.map { it.tags["outcome"] })
    }

    // The suffix or product lookup can fail before a session exists; that open still happened.
    @Test
    fun `an open that failed before its product was known is still reported as an error`() {
        telemetry.open().failedToStart()

        val event = captured.single()
        assertEquals("error", event.tags["outcome"])
        assertFalse(event.tags.containsKey("product.id"))
    }

    @Test
    fun `the first reveal reports what showed the page`() {
        val page = telemetry.open().apply { opened(HUMANITY) }
        now = LOADED_AT

        page.revealed(PageReveal.FALLBACK)
        page.revealed(PageReveal.THEME)

        val event = captured.single()
        assertEquals("host.webview.revealed" to "fallback", event.name to event.tags["reveal.via"])
        assertEquals("session-1", event.tags["host.session_id"])
    }

    @Test
    fun `a page that is not a product reports nothing`() {
        val page = telemetry.open().apply { notAProduct() }

        page.finished(loaded = true)
        page.revealed(PageReveal.THEME)
        page.closed()

        assertEquals(emptyList<HostEvent>(), captured)
    }

    private companion object {
        const val TAPPED_AT = 500L
        const val TAPPED_AT_EPOCH = 1_793_880_000_000L
        const val OPEN_REQUESTED_AT = 1_000L
        const val LOADED_AT = 1_750L
        val HUMANITY = ProductId.parse("humanity.dot").getOrThrow()
        val ORIGIN = HostEventOrigin(
            appVariant = "nightly",
            appFlavor = "gp",
            nativeAppVersion = "1.2.3",
            platform = "android",
            osVersion = "15",
            appBuild = 123L,
            launchId = "launch-7",
        )
    }
}
