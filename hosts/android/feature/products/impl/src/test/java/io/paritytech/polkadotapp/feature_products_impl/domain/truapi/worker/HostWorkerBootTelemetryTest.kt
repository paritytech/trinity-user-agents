package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.paritytech.polkadotapp.common.data.telemetry.HostCacheHits
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadTrigger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductId

class HostWorkerBootTelemetryTest {
    private var now = 0L
    private val events = mutableListOf<HostEvent>()
    private val cacheHits = HostCacheHits()
    private val telemetry = HostWorkerBootTelemetry(
        originProvider = { HostEventOrigin("nightly", "gp", "1.2.3", "android", "15", 123L, "launch-7") },
        timeProvider = { now },
        capture = { events += it() },
        cacheHits = cacheHits,
    )

    @Test
    fun `a boot reports each phase and its whole time since the core asked for the worker`() {
        val boot = telemetry.begin(HUMANITY, "worker-session", demandedAtMillis = 1_000L)
        now = 1_400L
        boot.phase(HostWorkerBootTelemetry.RESOLVE_MS, startedAtMillis = 1_100L)
        now = 1_500L
        boot.phase(HostWorkerBootTelemetry.RESOLVE_MS, startedAtMillis = 1_450L)

        boot.finish(HostEventOutcome.OK, endedAtMillis = 2_000L)

        val event = events.single()
        assertEquals("host.worker.boot", event.name)
        assertEquals(1_000L, event.durationMs)
        assertEquals(300L, event.extras[HostWorkerBootTelemetry.RESOLVE_MS])
        assertEquals("worker-session" to "humanity.dot", event.tags["host.session_id"] to event.tags["product.id"])
    }

    // Matches iOS: until the WebView object exists, then from there until its blank page is ready,
    // each only the first time.
    @Test
    fun `the WebView phases time its creation, then its blank page from the moment it existed`() {
        val boot = telemetry.begin(HUMANITY, "worker-session", demandedAtMillis = 1_000L)
        now = 1_250L
        boot.webViewCreated(createStartedAtMillis = 1_200L)
        now = 1_300L
        boot.webViewCreated(createStartedAtMillis = 1_280L)
        now = 1_700L
        boot.blankReady()
        now = 1_900L
        boot.blankReady()

        boot.finish(HostEventOutcome.OK, endedAtMillis = 2_000L)

        assertEquals(
            HostEvent(
                name = "host.worker.boot",
                tags = mapOf(
                    "host.platform" to "android",
                    "host.app_variant" to "nightly",
                    "host.app_flavor" to "gp",
                    "host.native_app_version" to "1.2.3",
                    "outcome" to "ok",
                    "host.os_version" to "15",
                    "host.session_id" to "worker-session",
                    "product.id" to "humanity.dot",
                ),
                extras = mapOf(
                    "host.app_build" to 123L,
                    "event.monotonic_ms" to 2_000L,
                    "duration_ms" to 1_000L,
                    "app.launch_id" to "launch-7",
                    "host.session_id" to "worker-session",
                    HostWorkerBootTelemetry.WEBVIEW_CREATE_MS to 50L,
                    HostWorkerBootTelemetry.BLANK_READY_MS to 450L,
                ),
                durationMs = 1_000L,
            ),
            events.single(),
        )
    }

    @Test
    fun `a blank page with no WebView created is not timed`() {
        val boot = telemetry.begin(HUMANITY, "worker-session", demandedAtMillis = 0L)

        boot.blankReady()
        boot.finish(HostEventOutcome.OK, endedAtMillis = 10L)

        assertFalse(events.single().extras.containsKey(HostWorkerBootTelemetry.BLANK_READY_MS))
    }

    @Test
    fun `an ended activation forgets what its session reported`() {
        val load = HostLoadContext(HostLoadTrigger.OPEN, "worker-session")
        cacheHits.isFirst("host.bulletin.fetch", "humanity.dot", load)

        telemetry.begin(HUMANITY, "worker-session", demandedAtMillis = 0L).ended()

        assertTrue(cacheHits.isFirst("host.bulletin.fetch", "humanity.dot", load))
    }

    @Test
    fun `a boot is reported once`() {
        val boot = telemetry.begin(HUMANITY, "worker-session", demandedAtMillis = 0L)

        boot.finish(HostEventOutcome.ABANDONED, endedAtMillis = 10L)
        boot.finish(HostEventOutcome.OK, endedAtMillis = 20L)

        assertEquals(listOf("abandoned"), events.map { it.tags["outcome"] })
    }

    private companion object {
        val HUMANITY = ProductId.parse("humanity.dot").getOrThrow()
    }
}
