package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class GestureDragThresholdTest {
    private val density = 2.625f
    private val systemSlop = 8f * density

    @Test
    fun `no setting is the low threshold, a number is dp, system keeps the system slop`() {
        assertEquals(GestureDragThreshold.DEFAULT_DP * density, GestureDragThreshold.slopPx(null, systemSlop, density))
        assertEquals(GestureDragThreshold.DEFAULT_DP * density, GestureDragThreshold.slopPx("", systemSlop, density))
        assertEquals(2.5f * density, GestureDragThreshold.slopPx("2.5", systemSlop, density))
        assertEquals(systemSlop, GestureDragThreshold.slopPx("system", systemSlop, density))
        assertEquals(GestureDragThreshold.DEFAULT_DP * density, GestureDragThreshold.slopPx("-1", systemSlop, density))
        assertEquals(GestureDragThreshold.DEFAULT_DP * density, GestureDragThreshold.slopPx("soon", systemSlop, density))
    }

    @Test
    fun `the low threshold is under a millimetre on a Pixel 9 and below the system slop`() {
        val pxPerMillimetre = 422f / 25.4f
        val low = GestureDragThreshold.slopPx(null, systemSlop, density)
        assertTrue(low / pxPerMillimetre < 1f)
        assertTrue(low < systemSlop)
    }

    @Test
    fun `a drag along the axis is claimed once past the low threshold, where the system slop still waits`() {
        val low = GestureDragThreshold.slopPx(null, systemSlop, density)
        val moved = low + 1f
        assertEquals(SlideDragDecision.CLAIM, GestureRules.decide(JsPanAxis.HORIZONTAL, moved, 0f, low))
        assertEquals(SlideDragDecision.WAIT, GestureRules.decide(JsPanAxis.HORIZONTAL, moved, 0f, systemSlop))
    }

    @Test
    fun `the catch-up feeds in exactly the lower threshold over three frames`() {
        val low = GestureDragThreshold.slopPx(null, systemSlop, density)
        assertEquals(low / 3f, GestureRules.caughtUp(low, 1), 0.0001f)
        assertEquals(low, GestureRules.caughtUp(low, GestureRules.CATCH_UP_FRAMES), 0.0001f)
    }
}
