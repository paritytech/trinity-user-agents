package io.paritytech.polkadotapp.feature_products_impl.domain.usecase

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostSessionRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import uniffi.truapi.ProductId

class HostProductResolvedTelemetryTest {
    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private val timeProvider = mockk<HostMonotonicTimeProvider> { every { nowMillis() } returns RESOLVED_AT }
    private val sessions = HostSessionRegistry()

    @Test
    fun `a resolved product reports how long the dotNS reads took, under the session it opened for`() {
        sessions.open("humanity.dot", "session-42")

        val event = reported { report(HUMANITY, STARTED_AT, resolved = true) }

        assertEquals(
            HostEvent(
                name = "host.product.resolved",
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
                    "event.monotonic_ms" to RESOLVED_AT,
                    "duration_ms" to RESOLVED_AT - STARTED_AT,
                    "host.session_id" to "session-42",
                ),
            ),
            event,
        )
    }

    @Test
    fun `a product resolved with no session open carries no session id`() {
        val event = reported { report(HUMANITY, STARTED_AT, resolved = true) }

        assertFalse(event.extras.containsKey("host.session_id"))
    }

    @Test
    fun `a failed resolution is reported as an error`() {
        val event = reported { report(HUMANITY, STARTED_AT, resolved = false) }

        assertEquals("error", event.tags["outcome"])
    }

    @Test
    fun `the event is not built when capture drops it`() {
        HostProductResolvedTelemetry(originProvider, timeProvider, HostEventCapture { }, sessions)
            .report(HUMANITY, STARTED_AT, resolved = true)

        verify(exactly = 0) { originProvider.current() }
        verify(exactly = 0) { timeProvider.nowMillis() }
    }

    private fun reported(action: HostProductResolvedTelemetry.() -> Unit): HostEvent {
        var captured: HostEvent? = null
        HostProductResolvedTelemetry(originProvider, timeProvider, HostEventCapture { captured = it() }, sessions)
            .action()
        return checkNotNull(captured) { "no event was captured" }
    }

    private companion object {
        const val STARTED_AT = 5_000L
        const val RESOLVED_AT = 5_420L
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
