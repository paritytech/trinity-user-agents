package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Offset
import io.paritytech.polkadotapp.feature_products_api.model.JsClearedShare
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskHaptic
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskHaptics
import io.paritytech.polkadotapp.feature_products_api.model.JsPanFeel
import io.paritytech.polkadotapp.feature_products_api.model.JsShareRescue
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.MaskShareWatch.Input
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MaskProgressWatchTest {
    private val share = JsClearedShare(900, "reached", rescue = JsShareRescue(1000, 500))
    private val haptics = JsMaskHaptics(100, JsMaskHaptic.TICK, JsMaskHaptic.SUCCESS)

    @Test
    fun `a mid stroke action is not repeated on the lift or a later added shape`() {
        val watch = MaskProgressWatch(Input())
        val live = share.copy(whileDown = true, rescue = null)
        assertTrue(watch.update(frame(0, 95, input = Input(down = true)), true, live, null).action)
        assertFalse(watch.update(frame(16, 98, input = Input(down = true)), true, live, null).action)
        assertFalse(watch.update(frame(32, 98, input = Input(lifts = 1)), true, live, null).action)
        assertFalse(watch.update(frame(48, 99, input = Input(lifts = 1, additions = 1)), true, live, null).action)
        assertTrue(watch.update(frame(64, 99, input = Input(down = true, lifts = 1, additions = 1)), true, live, null).action)
    }

    @Test
    fun `progress haptics do not turn a lift only share into a mid stroke action`() {
        val watch = MaskProgressWatch(Input())
        val liftedShare = share.copy(rescue = null)
        val down = watch.update(frame(0, 95, input = Input(down = true)), true, liftedShare, haptics)
        assertTrue(down.step)
        assertFalse(down.action)
        assertTrue(watch.update(frame(16, 95, input = Input(lifts = 1)), true, liftedShare, haptics).action)
    }

    @Test
    fun `a tree alone is silent and a new followed shape can check while still down`() {
        val watch = MaskProgressWatch(Input(down = true))
        val live = share.copy(whileDown = true, rescue = null)
        val tree = frame(0, 95, treeCovered = 95, input = Input(down = true, trees = 1))
        assertFalse(watch.update(tree.copy(hasFollowedShapes = false), true, live, null).action)
        assertTrue(watch.update(tree.copy(nowNanos = 16_000_000, hasFollowedShapes = true), true, live, null).action)
    }

    @Test
    fun `rescue starts at tree arrival and runs once per tree`() {
        val watch = MaskProgressWatch(Input())
        assertFalse(watch.update(frame(999, 60, treeCovered = 60), false, share, null).action)
        assertTrue(watch.update(frame(1000, 60, treeCovered = 60), false, share, null).action)
        assertFalse(watch.update(frame(2000, 60, treeCovered = 60), false, share, null).action)
        val next = frame(2500, 60, treeCovered = 60, input = Input(trees = 1)).copy(treeAtNanos = 2_000_000_000)
        assertFalse(watch.update(next, false, share, null).action)
        assertTrue(watch.update(next.copy(nowNanos = 3_000_000_000), false, share, null).action)
    }

    @Test
    fun `an unkeyed replacement resets rescue but resizing the same tree does not`() {
        val watch = MaskProgressWatch(Input())
        assertTrue(watch.update(frame(1000, 60, treeCovered = 60), false, share, null).action)
        val resized = frame(2000, 120, treeCovered = 120).copy(total = 200)
        assertFalse(watch.update(resized, false, share, null).action)
        val next = frame(2500, 60, treeCovered = 60).copy(treeAtNanos = 2_000_000_000)
        assertFalse(watch.update(next, false, share, null).action)
        assertTrue(watch.update(next.copy(nowNanos = 3_000_000_000), false, share, null).action)
    }

    @Test
    fun `rescue can fire with the finger down and leaves its eventual lift silent`() {
        val watch = MaskProgressWatch(Input())
        val down = frame(1000, 60, treeCovered = 60, input = Input(down = true))
        assertTrue(watch.update(down, true, share, null).action)
        assertFalse(watch.update(down.copy(input = Input(lifts = 1), covered = 95), true, share, null).action)
    }

    @Test
    fun `only newly covered samples restart rescue even when permille stays the same`() {
        val watch = MaskProgressWatch(Input())
        val start = frame(0, 6000, treeCovered = 6000).copy(total = 10000)
        assertFalse(watch.update(start, false, share, null).action)
        assertFalse(watch.update(start.copy(nowNanos = 900_000_000, covered = 6001, input = Input(additions = 1)), false, share, null).action)
        assertFalse(watch.update(start.copy(nowNanos = 1_100_000_000, covered = 6001, input = Input(additions = 2)), false, share, null).action)
        assertTrue(watch.update(start.copy(nowNanos = 1_900_000_000, covered = 6001, input = Input(additions = 2)), false, share, null).action)
    }

    @Test
    fun `rescue ignores cover below its floor or a floor above the target`() {
        val watch = MaskProgressWatch(Input())
        assertFalse(watch.update(frame(2000, 49, treeCovered = 49), false, share, null).action)
        assertEquals(null, watch.nextRescueAtNanos)
        val impossible = share.copy(rescue = JsShareRescue(0, 950))
        assertFalse(watch.update(frame(3000, 96, treeCovered = 96, input = Input(trees = 1)), false, impossible, null).action)
    }

    @Test
    fun `haptic steps start at the trees cover and coalesce multiple thresholds`() {
        val watch = MaskProgressWatch(Input())
        assertFalse(watch.update(frame(0, 24, treeCovered = 24), false, null, haptics).step)
        assertFalse(watch.update(frame(16, 25, treeCovered = 24), false, null, haptics).step)
        assertTrue(watch.update(frame(32, 70, treeCovered = 24), false, null, haptics).step)
        assertFalse(watch.update(frame(48, 70, treeCovered = 24), false, null, haptics).step)
        assertFalse(watch.update(frame(64, 90, treeCovered = 90, input = Input(trees = 1)), false, null, haptics).step)
    }

    @Test
    fun `zero step intervals and absent feedback play no step`() {
        val watch = MaskProgressWatch(Input())
        assertFalse(watch.update(frame(0, 60), false, null, haptics.copy(everyPermille = 0)).step)
        assertFalse(watch.update(frame(16, 90), false, null, null).step)
    }

    @Test
    fun `press scaling keeps the old shared scale unless a vertical scale is supplied`() {
        assertEquals(Offset(1.1f, 1.1f), GestureRules.pressScale(JsPanFeel(pressScale = 1.1f)))
        assertEquals(Offset(1f, 0.8f), GestureRules.pressScale(JsPanFeel(pressScale = 1f, pressScaleY = 0.8f)))
    }

    private fun frame(ms: Long, covered: Int, treeCovered: Int = 0, input: Input = Input()) =
        MaskProgressWatch.Frame(input, covered, 100, treeCovered, 0, ms * 1_000_000, hasFollowedShapes = true)
}
