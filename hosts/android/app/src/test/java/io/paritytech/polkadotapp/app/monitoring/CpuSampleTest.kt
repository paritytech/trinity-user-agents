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

    @Test
    fun `usage is 100 percent when the process kept its only core busy the whole interval`() {
        val previous = CpuSample(processCpuMillis = 10_000, elapsedRealtimeMillis = 100_000)
        val current = CpuSample(processCpuMillis = 70_000, elapsedRealtimeMillis = 160_000)

        assertEquals(100.0, current.usagePercentSince(previous, coreCount = 1), 0.0)
    }

    @Test
    fun `usage is 0 percent when the process used no CPU between two samples`() {
        val previous = CpuSample(processCpuMillis = 10_000, elapsedRealtimeMillis = 100_000)
        val current = CpuSample(processCpuMillis = 10_000, elapsedRealtimeMillis = 160_000)

        assertEquals(0.0, current.usagePercentSince(previous, coreCount = 4), 0.0)
    }
}
