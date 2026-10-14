package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.probe

import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainMetricReading
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.BlockProductionAnchor
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.BlockProductionAnchorSource
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.scoring.ChainHealthThresholds
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class BlockProductionProbeTest {
    @Test
    fun `the window is at least thirty seconds and at least ten block periods`() = runTest {
        val fast = collectReadings(blockTime = 2.seconds)
        runCurrent()
        assertEquals(15, fast.last().expectedBlocks)

        val slow = collectReadings(blockTime = 6.seconds)
        runCurrent()
        assertEquals(10, slow.last().expectedBlocks)
    }

    @Test
    fun `production is the height difference, not the number of heads delivered`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds)
        heads.emit(100)
        advanceTimeBy(60_000)
        heads.emit(110)
        runCurrent()

        assertEquals(10, readings.last().producedBlocks)
        assertEquals(1f, readings.last().share)
    }

    @Test
    fun `the share is the produced fraction of the expected blocks`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds)
        heads.emit(100)
        advanceTimeBy(60_000)
        heads.emit(109)
        runCurrent()

        assertEquals(9, readings.last().producedBlocks)
        assertEquals(0.9f, readings.last().share)
    }

    @Test
    fun `an unmeasurable window reports no share`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds)
        heads.emit(100)
        advanceTimeBy(30_000); runCurrent()

        assertNull(readings.last().share)
        assertNull(readings.last().producedBlocks)
    }

    @Test
    fun `a stall decays to zero once the window moves past the last head`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds)
        heads.emit(100)
        advanceTimeBy(60_000)
        heads.emit(110)
        runCurrent()
        assertEquals(1f, readings.last().share)

        advanceTimeBy(70_000); runCurrent()
        assertEquals(0f, readings.last().share)
        assertEquals(0, readings.last().producedBlocks)
    }

    @Test
    fun `more blocks than expected are capped`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds)
        heads.emit(100)
        advanceTimeBy(60_000)
        heads.emit(200)
        runCurrent()

        assertEquals(10, readings.last().producedBlocks)
        assertEquals(1f, readings.last().share)
    }

    @Test
    fun `an anchor flags a stalled chain without waiting out a window`() = runTest {
        // Ten blocks took ten windows of chain time, so only one of them belongs inside one window.
        val readings = collectReadings(blockTime = 6.seconds, anchor = anchorOf(headHeight = 1_000, chainClockSpan = 600.seconds))
        runCurrent()

        assertEquals(1, readings.last().producedBlocks)
        assertEquals(0.1f, readings.last().share)
    }

    @Test
    fun `an anchor on a chain running to time reports full production`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds, anchor = anchorOf(headHeight = 1_000, chainClockSpan = 60.seconds))
        runCurrent()

        assertEquals(1f, readings.last().share)
    }

    @Test
    fun `an anchor slower than the window seeds the fraction that fits inside it`() = runTest {
        val readings = collectReadings(blockTime = 2.seconds, anchor = anchorOf(headHeight = 1_000, chainClockSpan = 90.seconds))
        runCurrent()

        assertEquals(5, readings.last().producedBlocks)
        assertEquals(5f / 15f, readings.last().share)
    }

    @Test
    fun `a failed anchor leaves the window to fill from observation`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds, anchor = anchorOf { Result.failure(IllegalStateException("too short")) })
        runCurrent()
        assertNull(readings.last().share)

        heads.emit(100)
        advanceTimeBy(60_000)
        heads.emit(110)
        runCurrent()

        assertEquals(1f, readings.last().share)
    }

    @Test
    fun `an anchor that does not answer in time is given up`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds, anchor = anchorOf { CompletableDeferred<Result<BlockProductionAnchor>>().await() })
        advanceTimeBy(ChainHealthThresholds.BLOCK_PRODUCTION_ANCHOR_TIMEOUT.inWholeMilliseconds - 1); runCurrent()
        assertTrue(readings.last().anchorPending)

        advanceTimeBy(2); runCurrent()

        assertFalse(readings.last().anchorPending)
        assertNull(readings.last().share)
    }

    @Test
    fun `the anchor is reported as pending until the chain answers`() = runTest {
        val pending = CompletableDeferred<Result<BlockProductionAnchor>>()
        val readings = collectReadings(blockTime = 6.seconds, anchor = anchorOf { pending.await() })
        heads.emit(100)
        advanceTimeBy(5_000); runCurrent()
        assertTrue(readings.last().anchorPending)
        assertNull(readings.last().share)

        pending.complete(Result.success(BlockProductionAnchor(headHeight = 1_000, blocks = 10, chainClockSpan = 60.seconds)))
        runCurrent()

        assertFalse(readings.last().anchorPending)
        assertEquals(1f, readings.last().share)
    }

    @Test
    fun `heads seen while the chain is asked count once the anchor fails`() = runTest {
        val pending = CompletableDeferred<Result<BlockProductionAnchor>>()
        val readings = collectReadings(blockTime = 6.seconds, anchor = anchorOf { pending.await() })
        heads.emit(100)
        advanceTimeBy(10_000)
        pending.complete(Result.failure(IllegalStateException("too short")))
        advanceTimeBy(50_000)
        heads.emit(110)
        runCurrent()

        assertEquals(1f, readings.last().share)
    }

    @Test
    fun `losing the connection while the anchor is pending reports at once`() = runTest {
        val pending = CompletableDeferred<Result<BlockProductionAnchor>>()
        val readings = collectReadings(blockTime = 6.seconds, anchor = anchorOf { pending.await() })
        runCurrent()
        assertTrue(readings.last().anchorPending)

        connection.value = ChainConnectionPresentation.Offline
        runCurrent()

        assertFalse(readings.last().anchorPending)
        assertNull(readings.last().share)
    }

    @Test
    fun `while not connected the share is unknown at once`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds, initialConnection = ChainConnectionPresentation.Connecting)
        runCurrent()

        assertNull(readings.single().share)
    }

    @Test
    fun `a reconnect drops the heights the previous connection observed`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds)
        heads.emit(100)
        advanceTimeBy(60_000)
        heads.emit(101)
        runCurrent()
        assertEquals(0.1f, readings.last().share)

        connection.value = ChainConnectionPresentation.Connecting
        connection.value = ChainConnectionPresentation.Connected
        runCurrent()

        assertNull(readings.last().share)
    }

    @Test
    fun `a reconnect asks the chain again`() = runTest {
        var asked = 0
        val readings = collectReadings(blockTime = 6.seconds, anchor = anchorOf { asked++; Result.failure(IllegalStateException()) })
        runCurrent()

        connection.value = ChainConnectionPresentation.Connecting
        connection.value = ChainConnectionPresentation.Connected
        runCurrent()

        assertEquals(2, asked)
        assertNull(readings.last().share)
    }

    @Test
    fun `the expected blocks stay on the configured block time`() = runTest {
        val readings = collectReadings(blockTime = 6.seconds)
        repeat(4) { index ->
            heads.emit(index)
            advanceTimeBy(2_000); runCurrent()
        }

        assertEquals(10, readings.last().expectedBlocks)
    }

    private lateinit var heads: MutableSharedFlow<Int>
    private lateinit var connection: MutableStateFlow<ChainConnectionPresentation>

    private fun sampleTicks(): Flow<Unit> = flow {
        while (true) {
            emit(Unit)
            delay(ChainHealthThresholds.SAMPLE_TICK)
        }
    }

    private fun anchorOf(headHeight: Int, chainClockSpan: Duration) = BlockProductionAnchorSource { _, expectedBlocks ->
        Result.success(BlockProductionAnchor(headHeight = headHeight, blocks = expectedBlocks, chainClockSpan = chainClockSpan))
    }

    private fun anchorOf(fetch: suspend () -> Result<BlockProductionAnchor>) = BlockProductionAnchorSource { _, _ -> fetch() }

    private fun TestScope.collectReadings(
        blockTime: Duration,
        anchor: BlockProductionAnchorSource = anchorOf { Result.failure(IllegalStateException("no anchor")) },
        initialConnection: ChainConnectionPresentation = ChainConnectionPresentation.Connected,
    ): List<ChainMetricReading.BlockProduction> {
        heads = MutableSharedFlow(extraBufferCapacity = 64)
        connection = MutableStateFlow(initialConnection)
        val probe = BlockProductionProbe(FakeTimeProvider { testScheduler.currentTime }, anchor)
        val context = ChainMetricContext(
            chainId = "people",
            bestBlockNumber = heads,
            expectedBlockTime = blockTime,
            pendingRequests = emptyFlow(),
            connection = connection,
            ticks = sampleTicks(),
        )
        val results = mutableListOf<ChainMetricReading.BlockProduction>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            probe.observe(context).collect { results += it as ChainMetricReading.BlockProduction }
        }
        return results
    }
}
