package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class HostPageLoadedTelemetryTest {
    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private val timeProvider = mockk<HostMonotonicTimeProvider> { every { nowMillis() } returns LOADED_AT }

    @Test
    fun `reports the page load under its session with the time since the open was requested`() {
        val event = reported { report("session-42", HUMANITY, openRequestedAtMillis = OPEN_REQUESTED_AT) }

        assertEquals(
            HostEvent(
                name = "host.webview.page_loaded",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "nightly",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "outcome" to "ok",
                    "product.id" to "humanity.dot",
                ),
                extras = mapOf(
                    "host.app_build" to 123L,
                    "event.monotonic_ms" to LOADED_AT,
                    "duration_ms" to LOADED_AT - OPEN_REQUESTED_AT,
                    "host.session_id" to "session-42",
                ),
            ),
            event,
        )
    }

    @Test
    fun `the event is not built when capture drops it`() {
        HostPageLoadedTelemetry(originProvider, timeProvider, HostEventCapture { })
            .report("session-42", HUMANITY, OPEN_REQUESTED_AT)

        verify(exactly = 0) { originProvider.current() }
        verify(exactly = 0) { timeProvider.nowMillis() }
    }

    @Test
    fun `a failing capture does not reach the page load`() {
        HostPageLoadedTelemetry(originProvider, timeProvider, HostEventCapture { error("Sentry unavailable") })
            .report("session-42", HUMANITY, OPEN_REQUESTED_AT)
    }

    private fun reported(action: HostPageLoadedTelemetry.() -> Unit): HostEvent {
        var captured: HostEvent? = null
        HostPageLoadedTelemetry(originProvider, timeProvider, HostEventCapture { captured = it() }).action()
        return checkNotNull(captured) { "no event was captured" }
    }

    private companion object {
        const val OPEN_REQUESTED_AT = 10_000L
        const val LOADED_AT = 12_345L
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
