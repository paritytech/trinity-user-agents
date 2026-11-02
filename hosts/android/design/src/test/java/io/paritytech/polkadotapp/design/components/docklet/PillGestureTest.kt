package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.DurationUnit

class PillGestureTest {
    @Test
    fun `drag does not start when the finger moved 9 9 dp up`() {
        assertFalse(DockletGestures.swipeUpStarted(DpOffset(0.dp, (-9.9f).dp)))
    }

    @Test
    fun `drag starts when the finger moved 10 1 dp up`() {
        assertTrue(DockletGestures.swipeUpStarted(DpOffset(0.dp, (-10.1f).dp)))
    }

    @Test
    fun `drag does not start when the finger moved 30 dp sideways`() {
        assertFalse(DockletGestures.swipeUpStarted(DpOffset(30.dp, 0.dp)))
    }

    @Test
    fun `progress is distance over travel when the travel is 300 dp`() {
        assertEquals(0.5f, DockletGestures.swipeProgress(150.dp, Travel), EPS)
    }

    @Test
    fun `travel floors at 120 dp when the step-2 top is 100 dp away`() {
        assertEquals(0.5f, DockletGestures.swipeProgress(60.dp, 100.dp), EPS)
    }

    @Test
    fun `progress rubber-bands at a third when dragged past full travel`() {
        assertEquals(1.0667f, DockletGestures.swipeProgress(360.dp, Travel), EPS)
    }

    @Test
    fun `progress rubber-bands at a third when dragged below zero`() {
        assertEquals(-0.0333f, DockletGestures.swipeProgress((-30).dp, Travel), EPS)
    }

    @Test
    fun `opens when released at progress 0 5 with no velocity`() {
        assertEquals(DockletRelease.Open, DockletGestures.releaseSwipeUp(0.5f, 0f, Travel))
    }

    @Test
    fun `returns to rest when released at progress 0 49 with no velocity`() {
        assertEquals(DockletRelease.Close, DockletGestures.releaseSwipeUp(0.49f, 0f, Travel))
    }

    @Test
    fun `opens when moving up at 501 dp per second and reaching 0 5 within 100 ms`() {
        assertEquals(DockletRelease.Open, DockletGestures.releaseSwipeUp(0.4f, 501f, Travel))
    }

    @Test
    fun `returns to rest when moving up at 499 dp per second although 0 5 is 30 ms away`() {
        assertEquals(DockletRelease.Close, DockletGestures.releaseSwipeUp(0.45f, 499f, Travel))
    }

    @Test
    fun `returns to rest when moving up at 899 dp per second and 0 5 is 100 1 ms away`() {
        assertEquals(DockletRelease.Close, DockletGestures.releaseSwipeUp(0.2f, 899f, Travel))
    }

    @Test
    fun `opens when moving up at 901 dp per second and 0 5 is 99 9 ms away`() {
        assertEquals(DockletRelease.Open, DockletGestures.releaseSwipeUp(0.2f, 901f, Travel))
    }

    @Test
    fun `returns to rest when moving down fast at progress 0 45`() {
        assertEquals(DockletRelease.Close, DockletGestures.releaseSwipeUp(0.45f, -900f, Travel))
    }

    @Test
    fun `remaining motion takes 2 5 times distance over speed when released at 301 dp per second`() {
        assertMillis(332.2, DockletGestures.remainingMotion(40.dp, 301f))
    }

    @Test
    fun `remaining motion takes 340 ms when released at 299 dp per second`() {
        assertMillis(340.0, DockletGestures.remainingMotion(40.dp, 299f))
    }

    @Test
    fun `remaining motion clamps to 180 ms when the formula gives 50 ms`() {
        assertMillis(180.0, DockletGestures.remainingMotion(20.dp, 1000f))
    }

    @Test
    fun `remaining motion clamps to 420 ms when the formula gives 498 ms`() {
        assertMillis(420.0, DockletGestures.remainingMotion(99.6f.dp, 500f))
    }

    @Test
    fun `keyboard is asked for at 252 ms when the remaining motion lasts 420 ms`() {
        assertMillis(252.0, DockletGestures.keyboardRequestAt(420.milliseconds))
    }

    @Test
    fun `counts as a tap when the finger moved 13 9 dp`() {
        assertTrue(DockletGestures.isTap(13.9f.dp))
    }

    @Test
    fun `is not a tap when the finger moved 14 1 dp`() {
        assertFalse(DockletGestures.isTap(14.1f.dp))
    }

    @Test
    fun `closes when a swipe down is released at half the travel`() {
        assertEquals(DockletRelease.Close, DockletGestures.releaseSwipeDown(0.5f, 0f, DockletStep.Step1))
    }

    @Test
    fun `springs back to step 1 when a swipe down from step 1 is released at 49 percent`() {
        assertEquals(
            DockletRelease.SpringBack(DockletStep.Step1),
            DockletGestures.releaseSwipeDown(0.49f, 0f, DockletStep.Step1),
        )
    }

    @Test
    fun `springs back to step 2 when a swipe down from step 2 is released at 49 percent`() {
        assertEquals(
            DockletRelease.SpringBack(DockletStep.Step2),
            DockletGestures.releaseSwipeDown(0.49f, 0f, DockletStep.Step2),
        )
    }

    @Test
    fun `closes when flicked down at 501 dp per second from 10 percent`() {
        assertEquals(DockletRelease.Close, DockletGestures.releaseSwipeDown(0.1f, 501f, DockletStep.Step1))
    }

    @Test
    fun `springs back when flicked down at 499 dp per second from 10 percent`() {
        assertEquals(
            DockletRelease.SpringBack(DockletStep.Step1),
            DockletGestures.releaseSwipeDown(0.1f, 499f, DockletStep.Step1),
        )
    }

    @Test
    fun `springs back when flicked up at 900 dp per second from 49 percent`() {
        assertEquals(
            DockletRelease.SpringBack(DockletStep.Step2),
            DockletGestures.releaseSwipeDown(0.49f, -900f, DockletStep.Step2),
        )
    }

    @Test
    fun `cancel is not a release when the Pill owns the gesture`() {
        assertFalse(DockletGestures.isRelease(upConsumed = true, ownsGesture = true, dragging = false))
    }

    @Test
    fun `cancel is not a release when the card is being dragged`() {
        assertFalse(DockletGestures.isRelease(upConsumed = true, ownsGesture = false, dragging = true))
    }

    @Test
    fun `consumed up is a release when a child handled the touch`() {
        assertTrue(DockletGestures.isRelease(upConsumed = true, ownsGesture = false, dragging = false))
    }

    @Test
    fun `plain up is a release when the Pill owns the gesture`() {
        assertTrue(DockletGestures.isRelease(upConsumed = false, ownsGesture = true, dragging = false))
    }

    @Test
    fun `close fraction is 0 5 when the card is dragged half its travel`() {
        assertEquals(0.5f, DockletGestures.closedFraction(150f, 300f), EPS)
    }

    @Test
    fun `close fraction stops at 1 when the card is dragged past its travel`() {
        assertEquals(1f, DockletGestures.closedFraction(900f, 300f), EPS)
    }

    @Test
    fun `close fraction stops at 0 when the card is dragged back above its open position`() {
        assertEquals(0f, DockletGestures.closedFraction(-40f, 300f), EPS)
    }

    private fun assertMillis(expected: Double, actual: Duration) =
        assertEquals(expected, actual.toDouble(DurationUnit.MILLISECONDS), MILLIS_EPS)

    private companion object {
        const val EPS = 1e-3f
        const val MILLIS_EPS = 0.05
        val Travel = 300.dp
    }
}
