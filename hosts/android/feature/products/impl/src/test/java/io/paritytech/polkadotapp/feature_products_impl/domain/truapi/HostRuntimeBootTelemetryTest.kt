package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import org.junit.Assert.assertEquals
import org.junit.Test

class HostRuntimeBootTelemetryTest {
    private val events = mutableListOf<HostEvent>()
    private val telemetry = HostRuntimeBootTelemetry(
        originProvider = { HostEventOrigin("nightly", "gp", "1.2.3", "android", "15", 123L) },
        timeProvider = { BOOTED_AT },
        capture = { events += it() },
    )

    @Test
    fun `a boot reports how long the build took`() {
        telemetry.report(STARTED_AT, booted = true)

        val event = events.single()
        assertEquals("ok" to BOOTED_AT - STARTED_AT, event.tags["outcome"] to event.durationMs)
    }

    @Test
    fun `a boot that failed is reported as an error`() {
        telemetry.report(STARTED_AT, booted = false)

        assertEquals(listOf("error"), events.map { it.tags["outcome"] })
    }

    private companion object {
        const val STARTED_AT = 100L
        const val BOOTED_AT = 1_300L
    }
}
