package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class GestureMoveCoalescerTest {
    @Test
    fun `a burst sends only its newest move on the next frame`() {
        val moves = GestureMoveCoalescer<Int>()
        repeat(100) { moves.offer(it) }

        assertEquals(99, moves.takeFrame(1))
        assertNull(moves.takeFrame(2))
    }

    @Test
    fun `a slow reply retains only the newest pending position`() {
        val moves = GestureMoveCoalescer<Int>()
        moves.offer(1)
        assertEquals(1, moves.takeFrame(1))
        for (frame in 2L..100L) {
            moves.offer(frame.toInt())
            assertNull(moves.takeFrame(frame))
        }

        moves.acknowledge()
        assertEquals(100, moves.takeFrame(101))
        moves.acknowledge()
        assertNull(moves.takeFrame(102))
    }

    @Test
    fun `an immediate reply cannot send a second move in the same frame`() {
        val moves = GestureMoveCoalescer<Int>()
        moves.offer(1)
        assertEquals(1, moves.takeFrame(10))
        moves.acknowledge()
        moves.offer(2)

        assertNull(moves.takeFrame(10))
        assertEquals(2, moves.takeFrame(11))
    }

    @Test
    fun `clearing on release or cancellation discards pending work`() {
        val moves = GestureMoveCoalescer<Int>()
        moves.offer(1)
        moves.takeFrame(1)
        moves.offer(2)
        moves.clear()
        moves.acknowledge()

        assertNull(moves.takeFrame(2))
        moves.offer(3)
        assertEquals(3, moves.takeFrame(3))
    }

    @Test
    fun `an empty frame does not reserve a frame for a nonexistent move`() {
        val moves = GestureMoveCoalescer<Int>()
        assertNull(moves.takeFrame(1))
        moves.offer(1)

        assertEquals(1, moves.takeFrame(1))
    }

    @Test
    fun `clearing a gesture does not allow two moves in the same frame`() {
        val moves = GestureMoveCoalescer<Int>()
        moves.offer(1)
        moves.takeFrame(1)
        moves.clear()
        moves.offer(2)

        assertNull(moves.takeFrame(1))
        assertEquals(2, moves.takeFrame(2))
    }

    @Test
    fun `a touch sends its move at once and holds the next until the answer`() {
        val moves = GestureMoveCoalescer<Int>()
        moves.noteAnswer(0)
        moves.offer(1)
        assertEquals(1, moves.takeTouch(nowNanos = 100 * MS, minimumGapNanos = 10 * MS))

        moves.offer(2)
        assertNull(moves.takeTouch(nowNanos = 120 * MS, minimumGapNanos = 10 * MS))
        moves.noteAnswer(1)
        assertEquals(2, moves.takeTouch(nowNanos = 121 * MS, minimumGapNanos = 10 * MS))
    }

    @Test
    fun `an answered touch still waits out the gap before the next one`() {
        val moves = GestureMoveCoalescer<Int>()
        moves.noteAnswer(0)
        moves.offer(1)
        moves.takeTouch(nowNanos = 100 * MS, minimumGapNanos = 10 * MS)
        moves.noteAnswer(1)
        moves.offer(2)

        assertNull(moves.takeTouch(nowNanos = 105 * MS, minimumGapNanos = 10 * MS))
        assertEquals(2, moves.takeTouch(nowNanos = 110 * MS, minimumGapNanos = 10 * MS))
    }

    @Test
    fun `the frame after a touch's send does not send again in that frame`() {
        val moves = GestureMoveCoalescer<Int>()
        moves.noteAnswer(0)
        moves.offer(1)
        // The frame started at 100 ms; its touch was handled 1.5 ms later and sent.
        moves.takeTouch(nowNanos = 101 * MS + MS / 2, minimumGapNanos = 10 * MS)
        moves.noteAnswer(1)
        moves.offer(2)

        assertNull(moves.takeFrame(100 * MS))
        assertEquals(2, moves.takeFrame(117 * MS))
    }

    @Test
    fun `a move a touch held goes out in the next frame`() {
        val moves = GestureMoveCoalescer<Int>()
        moves.noteAnswer(0)
        moves.offer(1)
        moves.takeTouch(nowNanos = 100 * MS, minimumGapNanos = 10 * MS)
        moves.offer(2)
        assertNull(moves.takeTouch(nowNanos = 104 * MS, minimumGapNanos = 10 * MS))
        moves.noteAnswer(1)

        assertEquals(2, moves.takeFrame(117 * MS))
    }

    @Test
    fun `only a changed answer count acknowledges the move in flight`() {
        val moves = GestureMoveCoalescer<Int>()
        moves.noteAnswer(5)
        moves.offer(1)
        moves.takeFrame(1)
        moves.offer(2)

        moves.noteAnswer(5)
        assertNull(moves.takeFrame(2))
        moves.noteAnswer(6)
        assertEquals(2, moves.takeFrame(3))
    }

    private companion object {
        const val MS = 1_000_000L
    }
}
