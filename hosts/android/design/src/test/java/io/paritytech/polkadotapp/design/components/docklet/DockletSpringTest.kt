package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.SpringSpec
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.TestMonotonicFrameClock
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs
import kotlin.time.Duration.Companion.milliseconds

@OptIn(ExperimentalCoroutinesApi::class, ExperimentalTestApi::class)
class DockletSpringTest {
    @Test
    fun `a settle from rest takes about 450 ms`() = runTest {
        val millis = settleMillis(DockletSpring.Move, from = 0f, to = 1f, velocity = 0f)

        assertTrue("took $millis ms", millis in 420L..490L)
    }

    @Test
    fun `a flung settle reaches its pose in about 180 ms`() = runTest {
        val animatable = Animatable(0.6f, DockletSpring.Threshold)
        val start = currentTime
        var reached = -1L
        withContext(TestMonotonicFrameClock(this)) {
            animatable.animateTo(1f, DockletSpring.Flung, initialVelocity = 8f) {
                if (reached < 0 && value >= 1f - DockletSpring.Threshold) reached = currentTime - start
            }
        }

        assertTrue("reached at $reached ms", reached in 120L..260L)
    }

    @Test
    fun `the quiet hide takes about 570 ms`() = runTest {
        val millis = settleMillis(DockletSpring.Quiet, from = 0f, to = 1f, velocity = 0f)

        assertTrue("took $millis ms", millis in 540L..640L)
    }

    @Test
    fun `a release faster than 4 p per second uses the 0_78 damping`() {
        assertSame(DockletSpring.Flung, DockletSpring.afterRelease(4.5f))
        assertSame(DockletSpring.Flung, DockletSpring.afterRelease(-4.5f))
        assertSame(DockletSpring.Move, DockletSpring.afterRelease(3.9f))
    }

    @Test
    fun `a release hides when p plus 100 ms of velocity reaches half`() {
        assertEquals(1f, DockletSpring.hideTarget(0f, 0.45f, 0.5f, turnedBack = false), 0f)
        assertEquals(0f, DockletSpring.hideTarget(0f, 0.45f, 0.4f, turnedBack = false), 0f)
        assertEquals(1f, DockletSpring.hideTarget(0f, 0.2f, 3f, turnedBack = false), 0f)
    }

    @Test
    fun `a drag that came back by more than 30 percent returns to where it started`() {
        assertEquals(0f, DockletSpring.hideTarget(0f, 0.9f, 5f, turnedBack = true), 0f)
        assertEquals(1f, DockletSpring.hideTarget(1f, 0.1f, -5f, turnedBack = true), 0f)
    }

    @Test
    fun `a flick off the glow faster than 350 dp per second brings the dock back however short`() {
        assertEquals(0f, DockletSpring.restoreTarget(0.9f, 0f, fingerSpeed = 360f, turnedBack = false), 0f)
        assertEquals(1f, DockletSpring.restoreTarget(0.9f, 0f, fingerSpeed = 340f, turnedBack = false), 0f)
        assertEquals(0f, DockletSpring.restoreTarget(0.3f, 0f, fingerSpeed = 0f, turnedBack = false), 0f)
    }

    @Test
    fun `the half-way tick switches on at 0_54 and off under 0_46`() {
        assertFalse(DockletSpring.pastHalf(wasPast = false, projected = 0.53f))
        assertTrue(DockletSpring.pastHalf(wasPast = false, projected = 0.54f))
        assertTrue(DockletSpring.pastHalf(wasPast = true, projected = 0.47f))
        assertFalse(DockletSpring.pastHalf(wasPast = true, projected = 0.45f))
    }

    @Test
    fun `velocity is the change of p over the last 100 ms`() {
        val tracker = DockletReleaseTracker(0.milliseconds, 0f, DockletPoint(0f, 0f))
        listOf(50L to 0.1f, 100L to 0.2f, 150L to 0.3f).forEach { (time, p) -> tracker.add(time.milliseconds, p, DockletPoint(p * 300f, 0f)) }

        assertEquals(2f, tracker.velocity(travel = 300f), EPS)
    }

    @Test
    fun `a final pause slower than 150 dp per second is trimmed before the velocity`() {
        val tracker = DockletReleaseTracker(0.milliseconds, 0f, DockletPoint(0f, 0f))
        tracker.add(50.milliseconds, 0.25f, DockletPoint(75f, 0f))
        tracker.add(100.milliseconds, 0.5f, DockletPoint(150f, 0f))
        tracker.add(180.milliseconds, 0.501f, DockletPoint(150.3f, 0f))

        assertEquals(5f, tracker.velocity(travel = 300f), EPS)
    }

    @Test
    fun `the drag turned back once it lost more than 30 percent of its furthest point`() {
        val tracker = DockletReleaseTracker(0.milliseconds, 0f, DockletPoint(0f, 0f))
        tracker.add(50.milliseconds, 0.8f, DockletPoint(240f, 0f))
        tracker.add(100.milliseconds, 0.57f, DockletPoint(171f, 0f))
        assertFalse(tracker.turnedBack())

        tracker.add(150.milliseconds, 0.5f, DockletPoint(150f, 0f))
        assertTrue(tracker.turnedBack())
    }

    @Test
    fun `finger speed is distance over the last 100 ms`() {
        val tracker = DockletReleaseTracker(0.milliseconds, 1f, DockletPoint(400f, 500f))
        tracker.add(50.milliseconds, 0.9f, DockletPoint(370f, 540f))

        assertEquals(1000f, tracker.fingerSpeed(), 1f)
    }

    private suspend fun TestScope.settleMillis(spec: SpringSpec<Float>, from: Float, to: Float, velocity: Float): Long {
        val animatable = Animatable(from, DockletSpring.Threshold)
        val start = currentTime
        withContext(TestMonotonicFrameClock(this)) {
            animatable.animateTo(to, spec, initialVelocity = velocity)
        }
        assertTrue(abs(animatable.value - to) < DockletSpring.Threshold)
        return currentTime - start
    }

    private companion object {
        const val EPS = 1e-3f
    }
}
