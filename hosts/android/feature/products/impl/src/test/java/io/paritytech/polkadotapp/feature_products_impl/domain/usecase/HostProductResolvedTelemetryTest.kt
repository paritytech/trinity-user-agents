package io.paritytech.polkadotapp.feature_products_impl.domain.usecase

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostCacheHits
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadTrigger
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductId

class HostProductResolvedTelemetryTest {
    private val originProvider = mockk<HostEventOriginProvider> { every { current() } returns ORIGIN }
    private val timeProvider = mockk<HostMonotonicTimeProvider> { every { nowMillis() } returns RESOLVED_AT }
    private val cacheHits = HostCacheHits()

    @Test
    fun `a resolved product reports how long the dotNS reads took, under the session it opened for`() {
        val event = reported { report(HUMANITY, OPEN, STARTED_AT, QUEUE_MS, resolved = true) }

        assertEquals(
            HostEvent(
                name = "host.product.resolved",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "nightly",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "outcome" to "ok",
                    "host.os_version" to "15",
                    "product.id" to "humanity.dot",
                    "host.session_id" to "session-42",
                    "load.trigger" to "open",
                ),
                extras = mapOf(
                    "host.app_build" to 123L,
                    "event.monotonic_ms" to RESOLVED_AT,
                    "duration_ms" to RESOLVED_AT - STARTED_AT,
                    "app.launch_id" to "launch-7",
                    "host.session_id" to "session-42",
                    "queue_ms" to QUEUE_MS,
                ),
                durationMs = RESOLVED_AT - STARTED_AT,
            ),
            event,
        )
    }

    @Test
    fun `a product resolved for an open with no session carries no session id`() {
        val load = HostLoadContext(HostLoadTrigger.OPEN, sessionId = null)

        val event = reported { report(HUMANITY, load, STARTED_AT, QUEUE_MS, resolved = true) }

        assertFalse(event.tags.containsKey("host.session_id"))
    }

    @Test
    fun `a failed resolution is reported as an error`() {
        val event = reported { report(HUMANITY, OPEN, STARTED_AT, QUEUE_MS, resolved = false) }

        assertEquals("error", event.tags["outcome"])
    }

    @Test
    fun `the event is not built when capture drops it`() {
        HostProductResolvedTelemetry(originProvider, timeProvider, HostEventCapture { }, cacheHits)
            .report(HUMANITY, OPEN, STARTED_AT, QUEUE_MS, resolved = true)

        verify(exactly = 0) { originProvider.current() }
        verify(exactly = 0) { timeProvider.nowMillis() }
    }

    @Test
    fun `a prewarm is reported under its own trigger`() {
        val prewarm = HostLoadContext(HostLoadTrigger.PREWARM, sessionId = null)

        val event = reported { report(HUMANITY, prewarm, STARTED_AT, QUEUE_MS, resolved = true) }

        assertEquals("prewarm", event.tags["load.trigger"])
        assertFalse(event.tags.containsKey("host.session_id"))
    }

    @Test
    fun `a memory hit while capture is off is not counted as the session's one hit`() {
        val off = object : HostEventCapture {
            override val enabled = false

            override fun capture(event: () -> HostEvent) = error("nothing is captured while off")
        }

        HostProductResolvedTelemetry(originProvider, timeProvider, off, cacheHits)
            .reportMemoryHit(HUMANITY, OPEN, STARTED_AT, queueMillis = null)

        assertTrue(cacheHits.isFirst("host.product.resolved", "humanity.dot", OPEN))
    }

    private fun reported(action: HostProductResolvedTelemetry.() -> Unit): HostEvent {
        var captured: HostEvent? = null
        HostProductResolvedTelemetry(originProvider, timeProvider, HostEventCapture { captured = it() }, cacheHits)
            .action()
        return checkNotNull(captured) { "no event was captured" }
    }

    private companion object {
        const val STARTED_AT = 5_000L
        const val RESOLVED_AT = 5_420L
        const val QUEUE_MS = 140L
        val OPEN = HostLoadContext(HostLoadTrigger.OPEN, "session-42")
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
