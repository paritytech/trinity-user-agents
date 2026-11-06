package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class HostWorkerBootTelemetryTest {
    private var now = 0L
    private val events = mutableListOf<HostEvent>()
    private val telemetry = HostWorkerBootTelemetry(
        originProvider = { HostEventOrigin("nightly", "gp", "1.2.3", "android", "15", 123L) },
        timeProvider = { now },
        capture = { events += it() },
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
