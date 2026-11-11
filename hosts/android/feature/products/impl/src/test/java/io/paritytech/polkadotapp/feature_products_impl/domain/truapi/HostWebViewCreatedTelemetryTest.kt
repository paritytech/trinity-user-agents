package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
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

        reporter.report("bootstrap-session-42")

        assertEquals(HostWebViewCreatedTelemetry.EVENT_NAME, captured?.name)
        assertEquals("bootstrap-session-42", captured?.extras?.get("host.session_id"))
        assertEquals("nightly", captured?.tags?.get("host.app_variant"))
        assertEquals("gp", captured?.tags?.get("host.app_flavor"))
        assertEquals(123L, captured?.extras?.get("host.app_build"))
        assertEquals(456L, captured?.extras?.get("event.monotonic_ms"))
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

        reporter.report("release-session")

        assertFalse(originRead)
        assertFalse(timeRead)
    }

    @Test
    fun `does not throw when capture fails`() {
        val capture = mock(HostEventCapture::class.java)
        doThrow(IllegalStateException("Sentry unavailable")).`when`(capture).capture(any())
        val reporter = reporter(capture)

        reporter.report("session")

        verifyCaptureAttempted(capture)
    }

    private fun reporter(capture: HostEventCapture) = HostWebViewCreatedTelemetry(
        originProvider = HostEventOriginProvider { origin },
        timeProvider = HostMonotonicTimeProvider { 456L },
        capture = capture,
    )

    private fun verifyCaptureAttempted(capture: HostEventCapture) {
        verify(capture).capture(any())
    }
}
