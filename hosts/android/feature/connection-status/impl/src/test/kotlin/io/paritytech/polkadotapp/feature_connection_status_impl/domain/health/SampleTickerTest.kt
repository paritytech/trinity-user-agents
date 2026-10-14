package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class SampleTickerTest {
    @Test
    fun `a probe subscribing late evaluates at once instead of waiting out a period`() = runTest {
        val ticks = sharedSampleTicks(backgroundScope, 1.seconds)
        advanceTimeBy(2_500); runCurrent()

        var received = 0
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { ticks.collect { received++ } }
        runCurrent()

        assertEquals(1, received)
    }

    @Test
    fun `a slow probe does not hold up the cadence of a fast one`() = runTest {
        val ticks = sharedSampleTicks(backgroundScope, 1.seconds)

        var fast = 0
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { ticks.collect { fast++ } }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            ticks.collect { delay(10.seconds) }
        }
        runCurrent()

        advanceTimeBy(5_500); runCurrent()

        assertTrue("fast collector saw $fast ticks", fast >= 5)
    }
}
