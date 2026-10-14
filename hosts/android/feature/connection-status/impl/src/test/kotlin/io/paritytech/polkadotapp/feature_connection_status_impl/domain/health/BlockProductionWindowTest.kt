package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class BlockProductionWindowTest {
    private val window = BlockProductionWindow(window = 30.seconds)

    @Test
    fun `measures production as the height difference across the window`() {
        window.record(height = 100, at = at(0))
        window.record(height = 115, at = at(30))

        assertEquals(15, window.blocksProduced(at(30)))
    }

    @Test
    fun `an unmeasurable span is not a stalled chain`() {
        assertNull(window.blocksProduced(at(10)))

        window.record(height = 100, at = at(10))
        window.record(height = 101, at = at(15))

        assertNull(window.blocksProduced(at(15)))
    }

    @Test
    fun `a stall reads as nothing produced once the window has moved past the last head`() {
        window.record(height = 100, at = at(0))
        window.record(height = 115, at = at(30))

        assertEquals(0, window.blocksProduced(at(70)))
    }

    @Test
    fun `a chain that stayed at one height across the window produced nothing`() {
        window.record(height = 100, at = at(0))
        window.record(height = 100, at = at(30))

        assertEquals(0, window.blocksProduced(at(30)))
    }

    @Test
    fun `the sample at the window start is kept as the height the difference is measured from`() {
        window.record(height = 100, at = at(0))
        window.record(height = 110, at = at(20))
        window.record(height = 120, at = at(40))

        assertEquals(10, window.blocksProduced(at(50)))
    }

    @Test
    fun `a reorg counts from the anchor height`() {
        window.record(height = 10, at = at(0))
        window.record(height = 9, at = at(2))
        window.record(height = 11, at = at(4))

        assertEquals(1, window.blocksProduced(at(30)))
    }

    @Test
    fun `a head below the anchor reads as nothing produced rather than underflowing`() {
        window.record(height = 200, at = at(0))
        window.record(height = 150, at = at(30))

        assertEquals(0, window.blocksProduced(at(30)))
    }

    @Test
    fun `an anchor whose blocks fit the window seeds full production at once`() {
        window.seed(BlockProductionAnchor(headHeight = 30, blocks = 15, chainClockSpan = 30.seconds), at = at(100))

        assertEquals(15, window.blocksProduced(at(100)))
    }

    @Test
    fun `an anchor faster than the window still seeds full production`() {
        window.seed(BlockProductionAnchor(headHeight = 30, blocks = 15, chainClockSpan = 10.seconds), at = at(100))

        assertEquals(15, window.blocksProduced(at(100)))
    }

    @Test
    fun `an anchor slower than the window seeds the fraction that fits inside it`() {
        window.seed(BlockProductionAnchor(headHeight = 30, blocks = 15, chainClockSpan = 90.seconds), at = at(100))

        assertEquals(5, window.blocksProduced(at(100)))
    }

    @Test
    fun `an anchor far slower than the window seeds nothing produced`() {
        window.seed(BlockProductionAnchor(headHeight = 30, blocks = 15, chainClockSpan = 600.seconds), at = at(100))

        assertEquals(0, window.blocksProduced(at(100)))
    }

    @Test
    fun `an anchor on a chain shorter than the window counts from height zero`() {
        window.seed(BlockProductionAnchor(headHeight = 3, blocks = 15, chainClockSpan = 30.seconds), at = at(100))

        assertEquals(3, window.blocksProduced(at(100)))
    }

    @Test
    fun `heads arriving after an anchor continue the same window`() {
        window.seed(BlockProductionAnchor(headHeight = 10, blocks = 15, chainClockSpan = 30.seconds), at = at(100))
        window.record(height = 12, at = at(104))

        assertEquals(12, window.blocksProduced(at(104)))
    }

    @Test
    fun `seeding replaces whatever was observed before`() {
        window.record(height = 100, at = at(0))
        window.record(height = 100, at = at(30))
        window.seed(BlockProductionAnchor(headHeight = 1_000, blocks = 15, chainClockSpan = 30.seconds), at = at(30))

        assertEquals(15, window.blocksProduced(at(30)))
    }

    private fun at(seconds: Int): Instant = Instant.fromEpochMilliseconds(seconds * 1_000L)
}
