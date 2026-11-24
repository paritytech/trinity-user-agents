package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.feature_products_api.presentation.widget.CoverGridRules
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.BitSet

class CoverRevealWatchTest {
    private val reveal = JsCoverReveal(pan = "scratch", atPermille = 550, action = "scratch.reveal")

    /** The Daily Dollar card's grid: 35 x 37 cells, so 55% is 712.25 cells and 713 reach it. */
    private val cells = 35 * 37

    private fun touch(down: Boolean, lifts: Long) = JsGesturePosition.Touch(down, lifts)

    @Test
    fun `the share is reached at its cell and not one cell before, as the core counts it`() {
        assertFalse(reveal.reached(712, cells))
        assertTrue(reveal.reached(713, cells))
        val half = reveal.copy(atPermille = 500)
        assertFalse(half.reached(1, 4))
        assertTrue(half.reached(2, 4))
        assertTrue("zero is reached on any lift", reveal.copy(atPermille = 0).reached(0, 4))
        assertFalse("more than the whole is never reached", reveal.copy(atPermille = 1001).reached(4, 4))
        assertTrue("the largest grid does not overflow", reveal.copy(atPermille = 1000).reached(16_384, 16_384))
    }

    @Test
    fun `a lift past the share sends the reveal once`() {
        val watch = CoverRevealWatch()
        assertFalse(watch.update(touch(down = true, lifts = 0), 800, cells, reveal))
        assertTrue(watch.update(touch(down = false, lifts = 1), 800, cells, reveal))
        assertFalse(watch.update(touch(down = false, lifts = 1), 900, cells, reveal))
    }

    @Test
    fun `a lift one cell short sends nothing`() {
        assertFalse(CoverRevealWatch().update(touch(down = false, lifts = 1), 712, cells, reveal))
    }

    @Test
    fun `cells the worker clears in answer to the end, after the lift, count`() {
        val watch = CoverRevealWatch()
        assertFalse(watch.update(touch(down = false, lifts = 1), 700, cells, reveal))
        assertTrue(watch.update(touch(down = false, lifts = 1), 713, cells, reveal))
    }

    @Test
    fun `nothing is sent while the finger is down, and a new touch starts over`() {
        val watch = CoverRevealWatch()
        assertFalse(watch.update(touch(down = false, lifts = 1), 100, cells, reveal))
        assertFalse(watch.update(touch(down = true, lifts = 1), 800, cells, reveal))
        assertTrue(watch.update(touch(down = false, lifts = 2), 800, cells, reveal))
    }

    @Test
    fun `lifts before the cover was drawn send nothing`() {
        val watch = CoverRevealWatch(lifts = 3)
        assertFalse(watch.update(touch(down = false, lifts = 3), 900, cells, reveal))
        assertTrue(watch.update(touch(down = false, lifts = 4), 900, cells, reveal))
    }

    @Test
    fun `a touch counts its lifts and ignores a repeated state`() {
        val position = JsGesturePosition()
        position.touched(down = false)
        assertEquals(touch(down = false, lifts = 0), position.touch.value)
        position.touched(down = true)
        position.touched(down = true)
        position.touched(down = false)
        assertEquals(touch(down = false, lifts = 1), position.touch.value)
    }

    @Test
    fun `cells outside the grid are not counted`() {
        val cleared = BitSet().apply {
            set(3)
            set(cells)
        }
        assertEquals(1, CoverGridRules.countBelow(cleared, cells))
    }
}
