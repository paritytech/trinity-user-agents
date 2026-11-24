package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import io.paritytech.polkadotapp.feature_products_api.model.JsPanRelease
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTravel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GestureRulesTest {
    private val slop = 8f

    @Test
    fun `a horizontal pan claims a sideways drag in both directions and leaves an upright one to the surface`() {
        assertEquals(PanDecision.CLAIM, GestureRules.decide(JsPanAxis.HORIZONTAL, dx = 9f, dy = 2f, slop = slop))
        assertEquals(PanDecision.CLAIM, GestureRules.decide(JsPanAxis.HORIZONTAL, dx = -9f, dy = 2f, slop = slop))
        assertEquals(PanDecision.RELEASE, GestureRules.decide(JsPanAxis.HORIZONTAL, dx = 2f, dy = 9f, slop = slop))
        assertEquals(PanDecision.WAIT, GestureRules.decide(JsPanAxis.HORIZONTAL, dx = 5f, dy = 5f, slop = slop))
    }

    @Test
    fun `a vertical pan is the horizontal rule turned on its side`() {
        assertEquals(PanDecision.CLAIM, GestureRules.decide(JsPanAxis.VERTICAL, dx = 2f, dy = -9f, slop = slop))
        assertEquals(PanDecision.RELEASE, GestureRules.decide(JsPanAxis.VERTICAL, dx = 9f, dy = 2f, slop = slop))
    }

    @Test
    fun `a free pan claims any drag past the slop`() {
        assertEquals(PanDecision.CLAIM, GestureRules.decide(JsPanAxis.FREE, dx = 0f, dy = 9f, slop = slop))
        assertEquals(PanDecision.CLAIM, GestureRules.decide(JsPanAxis.FREE, dx = -9f, dy = 0f, slop = slop))
        assertEquals(PanDecision.WAIT, GestureRules.decide(JsPanAxis.FREE, dx = 8f, dy = 8f, slop = slop))
    }

    @Test
    fun `parent width travel is the node's width less its children's, never below zero`() {
        assertEquals(244f, GestureRules.travelPx(JsPanTravel.ParentWidth, nodeWidthPx = 300, childrenWidthPx = 56, dpToPx = 1f))
        assertEquals(244f, GestureRules.travelPx(null, nodeWidthPx = 300, childrenWidthPx = 56, dpToPx = 1f))
        assertEquals(0f, GestureRules.travelPx(JsPanTravel.ParentWidth, nodeWidthPx = 40, childrenWidthPx = 56, dpToPx = 1f))
        assertEquals(250f, GestureRules.travelPx(JsPanTravel.Points(100), nodeWidthPx = 300, childrenWidthPx = 56, dpToPx = 2.5f))
    }

    @Test
    fun `the slop crossed before a claim is caught up in equal shares over three frames, in its own direction`() {
        assertEquals(0f, GestureRules.caughtUp(slopPx = 21f, frame = 0))
        assertEquals(7f, GestureRules.caughtUp(slopPx = 21f, frame = 1))
        assertEquals(14f, GestureRules.caughtUp(slopPx = 21f, frame = 2))
        assertEquals(21f, GestureRules.caughtUp(slopPx = 21f, frame = 3))
        assertEquals(21f, GestureRules.caughtUp(slopPx = 21f, frame = 40))
        assertEquals(-7f, GestureRules.caughtUp(slopPx = -21f, frame = 1))
    }

    @Test
    fun `a place stays inside the travel`() {
        assertEquals(150f, GestureRules.place(start = 100f, delta = 50f, travelPx = 200f))
        assertEquals(200f, GestureRules.place(start = 100f, delta = 500f, travelPx = 200f))
        assertEquals(0f, GestureRules.place(start = 100f, delta = -500f, travelPx = 200f))
        assertEquals(0f, GestureRules.place(start = 0f, delta = 50f, travelPx = 0f))
    }

    @Test
    fun `progress is in thousandths, and zero travel reads zero`() {
        assertEquals(500, GestureRules.permille(100f, 200f))
        assertEquals(1000, GestureRules.permille(250f, 200f))
        assertEquals(0, GestureRules.permille(50f, 0f))
    }

    @Test
    fun `a complete release completes at its threshold and snaps back short of it`() {
        val release = JsPanRelease.Complete(thresholdPercent = 90)
        assertEquals(GestureSettle.COMPLETE, GestureRules.settle(release, permille = 900, travelPx = 200f))
        assertEquals(GestureSettle.SNAP_BACK, GestureRules.settle(release, permille = 899, travelPx = 200f))
    }

    @Test
    fun `zero travel never completes and the threshold is clamped to 1 through 100`() {
        assertEquals(GestureSettle.SNAP_BACK, GestureRules.settle(JsPanRelease.Complete(1), permille = 1000, travelPx = 0f))
        assertEquals(GestureSettle.COMPLETE, GestureRules.settle(JsPanRelease.Complete(0), permille = 10, travelPx = 200f))
        assertEquals(GestureSettle.SNAP_BACK, GestureRules.settle(JsPanRelease.Complete(0), permille = 9, travelPx = 200f))
        assertEquals(GestureSettle.COMPLETE, GestureRules.settle(JsPanRelease.Complete(250), permille = 1000, travelPx = 200f))
        assertEquals(GestureSettle.SNAP_BACK, GestureRules.settle(JsPanRelease.Complete(250), permille = 999, travelPx = 200f))
    }

    @Test
    fun `snap back and stay ignore how far the drag went`() {
        assertEquals(GestureSettle.SNAP_BACK, GestureRules.settle(JsPanRelease.SnapBack, permille = 1000, travelPx = 200f))
        assertEquals(GestureSettle.STAY, GestureRules.settle(JsPanRelease.Stay, permille = 300, travelPx = 200f))
    }

    @Test
    fun `moves are throttled by their interval, and zero sends every move`() {
        assertTrue(GestureRules.movesNow(nowMs = 0, lastSentMs = null, intervalMs = 50))
        assertFalse(GestureRules.movesNow(nowMs = 149, lastSentMs = 100, intervalMs = 50))
        assertTrue(GestureRules.movesNow(nowMs = 150, lastSentMs = 100, intervalMs = 50))
        assertTrue(GestureRules.movesNow(nowMs = 100, lastSentMs = 100, intervalMs = 0))
    }

    @Test
    fun `a disabled node or a drag held at the end takes no gesture`() {
        assertTrue(GestureRules.accepts(enabled = true, heldAtEnd = false))
        assertFalse(GestureRules.accepts(enabled = false, heldAtEnd = false))
        assertFalse(GestureRules.accepts(enabled = true, heldAtEnd = true))
    }
}
