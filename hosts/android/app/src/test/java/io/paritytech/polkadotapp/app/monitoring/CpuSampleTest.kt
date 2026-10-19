package io.paritytech.polkadotapp.app.monitoring

import org.junit.Assert.assertEquals
import org.junit.Test

class CpuSampleTest {
    @Test
    fun `usage is the share of all cores the process kept busy between two samples`() {
        val previous = CpuSample(processCpuMillis = 10_000, elapsedRealtimeMillis = 100_000)
        val current = CpuSample(processCpuMillis = 40_000, elapsedRealtimeMillis = 160_000)

        assertEquals(12.5, current.usagePercentSince(previous, coreCount = 4), 0.0)
    }
}
