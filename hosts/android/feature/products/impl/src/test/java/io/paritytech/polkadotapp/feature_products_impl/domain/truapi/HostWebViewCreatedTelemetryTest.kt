package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenSource
import io.paritytech.polkadotapp.common.data.telemetry.HostPageOpen
import io.paritytech.polkadotapp.common.data.telemetry.HostPageSession
import io.paritytech.polkadotapp.test_shared.any
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.mockito.Mockito.doThrow
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify

class HostWebViewCreatedTelemetryTest {
    private val origin = HostEventOrigin(
        appVariant = "nightly",
        appFlavor = "gp",
        nativeAppVersion = "1.2.3",
        platform = "android",
        osVersion = "15",
        appBuild = 123L,
    )

    @Test
    fun `reports the WebView event with the shared host tags when a WebView is created`() {
        var captured: HostEvent? = null
        val reporter = reporter(capture = HostEventCapture { captured = it() })

        reporter.report(session("bootstrap-session-42"))

        assertEquals(HostWebViewCreatedTelemetry.EVENT_NAME, captured?.name)
        assertEquals("bootstrap-session-42", captured?.tags?.get("host.session_id"))
        assertEquals("nightly", captured?.tags?.get("host.app_variant"))
        assertEquals("gp", captured?.tags?.get("host.app_flavor"))
        assertEquals(123L, captured?.extras?.get("host.app_build"))
        assertEquals(456L, captured?.extras?.get("event.monotonic_ms"))
    }

    @Test
    fun `tags the open source and reports the time since the tap when the page was opened by one`() {
        var captured: HostEvent? = null
        val reporter = reporter(capture = HostEventCapture { captured = it() })

        reporter.report(
            session(
                "bootstrap-session-42",
                HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L, monotonicMs = 100L, sessionId = "open-session"),
            ),
        )

        assertEquals("pocket_card", captured?.tags?.get("open.source"))
        assertEquals(356L, captured?.extras?.get("tap_ms"))
    }

    @Test
    fun `times the tap on the monotonic clock, so a wall clock change cannot make it negative`() {
        var captured: HostEvent? = null
        val reporter = reporter(capture = HostEventCapture { captured = it() })
        // The wall clock was set back after the tap, so its epoch reading is after now's.
        val tap = HostPageOpen(
            HostOpenSource.POCKET_CARD,
            epochMs = System.currentTimeMillis() + 3_600_000L,
            monotonicMs = 400L,
            sessionId = "open-session",
        )

        reporter.report(session("bootstrap-session-42", tap))
        assertEquals(56L, captured?.extras?.get("tap_ms"))

        reporter.reportVisible(session("bootstrap-session-42", tap), webViewCreatedAtMs = 420L)
        assertEquals(56L, captured?.extras?.get("tap_ms"))
        assertEquals(36L, captured?.extras?.get("webview_ms"))
    }

    @Test
    fun `leaves the tap time out rather than sending a negative or clamped one`() {
        var captured: HostEvent? = null
        val reporter = reporter(capture = HostEventCapture { captured = it() })

        reporter.report(
            session(
                "bootstrap-session-42",
                HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L, monotonicMs = 900L, sessionId = "open-session"),
            ),
        )

        assertEquals("pocket_card", captured?.tags?.get("open.source"))
        assertFalse(captured?.extras.orEmpty().containsKey("tap_ms"))
    }

    @Test
    fun `reports the first visible commit with the WebView time and no tap when nothing asked for the page`() {
        var captured: HostEvent? = null
        val reporter = reporter(capture = HostEventCapture { captured = it() })

        reporter.reportVisible(session("bootstrap-session-42"), webViewCreatedAtMs = 100L)

        assertEquals("host.page.visible", captured?.name)
        assertEquals("bootstrap-session-42", captured?.tags?.get("host.session_id"))
        assertEquals(null, captured?.tags?.get("open.source"))
        assertEquals(356L, captured?.extras?.get("webview_ms"))
        assertEquals(null, captured?.extras?.get("tap_ms"))
    }

    @Test
    fun `reports the time since the tap and its source on the visible commit when the page was opened by one`() {
        var captured: HostEvent? = null
        val reporter = reporter(capture = HostEventCapture { captured = it() })

        reporter.reportVisible(
            session(
                "bootstrap-session-42",
                HostPageOpen(HostOpenSource.DEEPLINK, epochMs = 1_000L, monotonicMs = 6L, sessionId = "open-session"),
            ),
            webViewCreatedAtMs = 100L,
        )

        assertEquals("deeplink", captured?.tags?.get("open.source"))
        assertEquals(450L, captured?.extras?.get("tap_ms"))
        assertEquals(356L, captured?.extras?.get("webview_ms"))
    }

    @Test
    fun `tags a retried load on both events, and only a retried one`() {
        val captured = mutableListOf<HostEvent>()
        val reporter = reporter(capture = HostEventCapture { captured += it() })
        val tap = HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L, monotonicMs = 100L, sessionId = "tap-session")

        reporter.report(session("tap-session", tap, isRetry = true))
        reporter.reportVisible(session("tap-session", tap, isRetry = true), webViewCreatedAtMs = 400L)
        reporter.report(session("tap-session", tap))

        assertEquals(listOf("true", "true", null), captured.map { it.tags["open.retry"] })
        assertEquals(listOf("tap-session", "tap-session", "tap-session"), captured.map { it.tags["host.session_id"] })
        assertEquals(356L, captured[1].extras["tap_ms"])
    }

    @Test
    fun `keeps WebView metadata lazy when capture is disabled`() {
        var originRead = false
        var timeRead = false
        val reporter = HostWebViewCreatedTelemetry(
            originProvider = HostEventOriginProvider {
                originRead = true
                origin
            },
            timeProvider = HostMonotonicTimeProvider {
                timeRead = true
                456L
            },
            capture = HostEventCapture { },
        )

        reporter.report(session("release-session"))

        assertFalse(originRead)
        assertFalse(timeRead)
    }

    @Test
    fun `does not throw when capture fails`() {
        val capture = mock(HostEventCapture::class.java)
        doThrow(IllegalStateException("Sentry unavailable")).`when`(capture).capture(any())
        val reporter = reporter(capture)

        reporter.report(session("session"))

        verifyCaptureAttempted(capture)
    }

    private fun session(id: String, open: HostPageOpen? = null, isRetry: Boolean = false) =
        HostPageSession(productId = "product.paseo", id = id, open = open, isRetry = isRetry)

    private fun reporter(capture: HostEventCapture) = HostWebViewCreatedTelemetry(
        originProvider = HostEventOriginProvider { origin },
        timeProvider = HostMonotonicTimeProvider { 456L },
        capture = capture,
    )

    private fun verifyCaptureAttempted(capture: HostEventCapture) {
        verify(capture).capture(any())
    }
}
