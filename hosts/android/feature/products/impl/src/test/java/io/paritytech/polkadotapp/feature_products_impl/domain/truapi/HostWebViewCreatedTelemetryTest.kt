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
import uniffi.truapi.truapiHostVersion

class HostWebViewCreatedTelemetryTest {
    private val origin = HostEventOrigin(
        appVariant = "nightly",
        appFlavor = "gp",
        nativeAppVersion = "1.2.3",
        platform = "android",
        osVersion = "15",
        appBuild = 123L,
        launchId = "launch-7",
    )

    @Test
    fun `reports the WebView event with the shared host tags when a WebView is created`() {
        var captured: HostEvent? = null
        val reporter = reporter(capture = HostEventCapture { captured = it() })

        reporter.report("bootstrap-session-42")

        assertEquals(
            HostEvent(
                name = "host.webview.created",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "nightly",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "truapi.host_version" to truapiHostVersion(),
                    "host.session_id" to "bootstrap-session-42",
                ),
                extras = mapOf(
                    "host.app_build" to 123L,
                    "host.session_id" to "bootstrap-session-42",
                    "event.monotonic_ms" to 456L,
                ),
            ),
            captured,
        )
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
