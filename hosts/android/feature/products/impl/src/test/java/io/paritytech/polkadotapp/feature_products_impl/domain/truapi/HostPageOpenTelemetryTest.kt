package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostPocketTaps
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.PageReveal
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import uniffi.truapi.ProductId

class HostPageOpenTelemetryTest {
    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private var now = TAPPED_AT
    private val timeProvider = HostMonotonicTimeProvider { now }
    private val captured = mutableListOf<HostEvent>()
    private val capture = HostEventCapture { captured += it() }
    private val taps = HostPocketTaps(timeProvider, { TAPPED_AT_EPOCH }, capture)
    private val telemetry = HostPageOpenTelemetry(originProvider, timeProvider, capture, taps)

    @Test
    fun `reports the page load under its session, timed from the open and from the card tap`() {
        taps.tapped("humanity.dot")
        now = OPEN_REQUESTED_AT
        val page = telemetry.open()
        page.opened(HUMANITY, "session-42")
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
                    "host.session_id" to "session-42",
                    "product.id" to "humanity.dot",
                ),
                extras = mapOf(
                    "host.app_build" to 123L,
                    "event.monotonic_ms" to LOADED_AT,
                    "duration_ms" to LOADED_AT - OPEN_REQUESTED_AT,
                    "host.session_id" to "session-42",
                    "tap_ms" to LOADED_AT - TAPPED_AT,
                ),
                durationMs = LOADED_AT - OPEN_REQUESTED_AT,
            ),
            captured.single { it.name == "host.webview.page_loaded" },
        )
    }

    @Test
    fun `the tap that opens a page is reported as opened, and handed to the page`() {
        taps.tapped("humanity.dot")
        now = OPEN_REQUESTED_AT
        val page = telemetry.open()

        page.opened(HUMANITY, "session-42")

        val tapped = captured.single()
        assertEquals("host.pocket.card_tapped" to "opened", tapped.name to tapped.tags["outcome"])
        assertEquals("pocket_card", tapped.tags["open.source"])
        assertEquals("session-42" to "humanity.dot", tapped.tags["host.session_id"] to tapped.tags["product.id"])
        assertEquals(OPEN_REQUESTED_AT - TAPPED_AT, tapped.durationMs)
        assertEquals(TAPPED_AT_EPOCH to "pocket_card", page.pageOrigin?.openedAtEpochMs to page.pageOrigin?.openSource)
    }

    @Test
    fun `a page a link opened is handed the link's time as a deeplink open`() {
        val link = taps.linkHandled()
        taps.linked("humanity.dot", link)
        val page = telemetry.open()

        page.opened(HUMANITY, "session-42")

        assertEquals(TAPPED_AT_EPOCH to "deeplink", page.pageOrigin?.openedAtEpochMs to page.pageOrigin?.openSource)
        val tapped = captured.single()
        assertEquals("opened" to "deeplink", tapped.tags["outcome"] to tapped.tags["open.source"])
    }

    // A held page shown again starts no load, so without this the tap would go unreported.
    @Test
    fun `a tap that shows a held page again is reported as reused, timed to the page being visible`() {
        val page = telemetry.open().apply { opened(HUMANITY, "session-42") }
        now = 5_000L
        taps.tapped("humanity.dot")

        val tap = checkNotNull(page.reuseTap())
        now = 5_120L
        page.reused(tap)

        val reused = captured.single()
        assertEquals("host.pocket.card_tapped" to "reused", reused.name to reused.tags["outcome"])
        assertEquals(120L, reused.extras["reuse_ms"])
        assertEquals("session-42", reused.tags["host.session_id"])
    }

    @Test
    fun `a page opened without a tap hands the page no open time`() {
        val page = telemetry.open()

        page.opened(HUMANITY, "session-42")

        assertEquals(null, page.pageOrigin)
        assertEquals(emptyList<HostEvent>(), captured)
    }

    @Test
    fun `a page opened without a card tap carries no tap time`() {
        val page = telemetry.open()
        page.opened(HUMANITY, "session-42")

        page.finished(loaded = true)

        assertFalse(captured.single().extras.containsKey("tap_ms"))
    }

    @Test
    fun `a page whose main document failed is reported as an error`() {
        val page = telemetry.open().apply { opened(HUMANITY, "session-42") }

        page.finished(loaded = false)

        assertEquals(listOf("error"), captured.map { it.tags["outcome"] })
    }

    // Without it, a page the user gave up on is missing from the data rather than counted as slow.
    @Test
    fun `a page closed before it finished loading is reported as abandoned, and only that`() {
        val page = telemetry.open().apply { opened(HUMANITY, "session-42") }

        page.closed()
        page.finished(loaded = true)

        assertEquals(listOf("abandoned"), captured.map { it.tags["outcome"] })
    }

    @Test
    fun `closing a page that already loaded reports nothing more`() {
        val page = telemetry.open().apply { opened(HUMANITY, "session-42") }

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
        val page = telemetry.open().apply { opened(HUMANITY, "session-42") }
        now = LOADED_AT

        page.revealed(PageReveal.FALLBACK)
        page.revealed(PageReveal.THEME)

        val event = captured.single()
        assertEquals("host.webview.revealed" to "fallback", event.name to event.tags["via"])
        assertEquals("session-42", event.tags["host.session_id"])
    }

    @Test
    fun `a page that is not a product reports nothing`() {
        val page = telemetry.open().apply { notAProduct() }

        page.finished(loaded = true)
        page.revealed(PageReveal.THEME)
        page.closed()

        assertEquals(emptyList<HostEvent>(), captured)
    }

    @Test
    fun `the event is not built when capture drops it`() {
        val page = HostPageOpenTelemetry(originProvider, timeProvider, HostEventCapture { }, taps).open()

        page.opened(HUMANITY, "session-42")
        page.finished(loaded = true)

        verify(exactly = 0) { originProvider.current() }
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
        )
    }
}
