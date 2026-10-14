package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.probe

import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainMetricReading
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.scoring.ChainHealthThresholds
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class UnansweredRequestProbeTest {
    @Test
    fun `the limit is three block times`() = runTest {
        val readings = collectReadings()
        runCurrent()

        assertEquals(6.seconds, readings.last().limit)
    }

    @Test
    fun `a request answered within three block times keeps the node answering`() = runTest {
        val readings = collectReadings()
        pending.value = setOf(Any())
        advanceTimeBy(5_500); runCurrent()

        assertFalse(readings.last().isOverLimit)
    }

    @Test
    fun `a request left unanswered for more than three block times means the node is silent`() = runTest {
        val readings = collectReadings()
        val request = Any()
        pending.value = setOf(request)
        advanceTimeBy(6_500); runCurrent()
        assertFalse(readings.last().isOverLimit)

        advanceTimeBy(1_000); runCurrent()
        assertTrue(readings.last().isOverLimit)

        pending.value = emptySet()
        advanceTimeBy(1_000); runCurrent()
        assertFalse(readings.last().isOverLimit)
    }

    @Test
    fun `readings are reported only when the limit is crossed`() = runTest {
        val readings = collectReadings()
        pending.value = setOf(Any())
        advanceTimeBy(20_000); runCurrent()

        assertEquals(2, readings.size)
    }

    private lateinit var pending: MutableStateFlow<Set<Any>>

    private fun sampleTicks(): Flow<Unit> = flow {
        while (true) {
            emit(Unit)
            delay(ChainHealthThresholds.SAMPLE_TICK)
        }
    }

    private fun TestScope.collectReadings(): List<ChainMetricReading.UnansweredRequest> {
        pending = MutableStateFlow(emptySet())
        val probe = UnansweredRequestProbe(FakeTimeProvider { testScheduler.currentTime })
        val context = ChainMetricContext(
            chainId = "people",
            bestBlockNumber = emptyFlow(),
            expectedBlockTime = 2.seconds,
            pendingRequests = pending,
            connection = flowOf(ChainConnectionPresentation.Connected),
            ticks = sampleTicks(),
        )
        val results = mutableListOf<ChainMetricReading.UnansweredRequest>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            probe.observe(context).collect { results += it as ChainMetricReading.UnansweredRequest }
        }
        return results
    }
}
