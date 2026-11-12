package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.TestMonotonicFrameClock
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds

@OptIn(ExperimentalCoroutinesApi::class, ExperimentalTestApi::class)
class DockletHideStateTest {
    private val bridge = DockletGeometry.dockFrame(402.dp, 874.dp, 34.dp, threeButtonNavigation = false)
        .let { DockletDrawbridge(it, 402.dp, 874.dp) }
    private val landings = mutableListOf<DockletLanding>()
    private val moments = mutableListOf<DockletMoment>()

    @Test
    fun `a swipe released past half hides the dock with a heavy haptic`() = runTest {
        val state = hideState()

        swipe(state, fromX = 50f, toX = 250f)
        state.releaseHide()
        advanceUntilIdle()

        assertTrue(state.countsAsHidden)
        assertEquals(90f, state.pose().angleDegrees, EPS)
        assertEquals(listOf(DockletLanding.HiddenBySwipe), landings)
        assertTrue(DockletMoment.Hidden in moments)
    }

    @Test
    fun `a swipe released before half settles back without a landing`() = runTest {
        val state = hideState()

        swipe(state, fromX = 50f, toX = 100f, stepMillis = 120L)
        state.releaseHide()
        advanceUntilIdle()

        assertFalse(state.countsAsHidden)
        assertTrue(state.isShownAtRest)
        assertEquals(emptyList<DockletLanding>(), landings)
    }

    @Test
    fun `the dock counts as hidden from the moment the settle starts`() = runTest {
        val state = hideState()

        swipe(state, fromX = 50f, toX = 250f)
        state.releaseHide()

        assertTrue(state.countsAsHidden)
        assertTrue(state.isMoving)
    }

    @Test
    fun `a touch on the moving dock catches it where it is`() = runTest {
        val state = hideState()
        swipe(state, fromX = 50f, toX = 250f)
        state.releaseHide()
        advanceTimeBy(FrameMillis * 6)
        runCurrent()
        val before = state.pose()

        state.startHide(bridge.map(DockletPoint(200f, 816f), before), time = 1_000.milliseconds)
        runCurrent()

        assertEquals(before.angleDegrees, state.pose().angleDegrees, 0.5f)
        assertEquals(before.dx, state.pose().dx, 0.5f)
    }

    @Test
    fun `a pull from the glow released low and far from the edge brings the dock back`() = runTest {
        val state = hiddenState()

        state.startRestore(DockletPoint(395f, 500f), time = 0.milliseconds)
        listOf(100L to DockletPoint(330f, 650f), 200L to DockletPoint(200f, 860f), 300L to DockletPoint(150f, 865f))
            .forEach { (time, point) -> state.moveRestore(point, time.milliseconds) }
        state.releaseRestore()
        advanceUntilIdle()

        assertTrue(state.isShownAtRest)
        assertEquals(DockletLanding.Shown, landings.last())
        assertTrue(DockletMoment.Restored in moments)
    }

    @Test
    fun `a cancelled hold lands on the nearer pose`() = runTest {
        val state = hideState()

        swipe(state, fromX = 50f, toX = 280f, stepMillis = 200L)
        state.cancelDrag()
        advanceUntilIdle()

        assertTrue(state.countsAsHidden)
        assertEquals(90f, state.pose().angleDegrees, EPS)
    }

    @Test
    fun `the peek turns the hidden dock out and back`() = runTest {
        val state = hiddenState()

        state.peek()
        advanceTimeBy(700L + 200L)
        runCurrent()
        val middle = state.pose().angleDegrees
        advanceUntilIdle()

        assertTrue("angle $middle", middle < 90f - 5f)
        assertEquals(90f, state.pose().angleDegrees, EPS)
    }

    @Test
    fun `a touch on the dock cancels a pending peek`() = runTest {
        val state = hiddenState()

        state.peek()
        advanceTimeBy(300L)
        state.startRestore(DockletPoint(395f, 500f), time = 0.milliseconds)
        state.cancelDrag()
        advanceTimeBy(700L + 200L)
        runCurrent()

        assertEquals(90f, state.pose().angleDegrees, EPS)
    }

    @Test
    fun `the quiet hide lands without a haptic and fades the contents`() = runTest {
        val state = hideState()

        state.quietHide()
        advanceTimeBy(FrameMillis * 12)
        runCurrent()
        val midFade = state.quietFade()
        advanceUntilIdle()

        assertTrue(midFade > 0f)
        assertEquals(listOf(DockletLanding.HiddenQuietly), landings)
        assertEquals(emptyList<DockletMoment>(), moments)
    }

    @Test
    fun `the quiet hide does nothing while the dock is held`() = runTest {
        val state = hideState()
        state.startHide(DockletPoint(200f, 816f), time = 0.milliseconds)

        state.quietHide()
        advanceUntilIdle()

        assertFalse(state.countsAsHidden)
    }

    @Test
    fun `a tuck hides the dock at once with no landing, haptic or fade`() = runTest {
        val state = hideState()

        state.tuck()

        assertTrue(state.isOffScreen)
        assertEquals(90f, state.pose().angleDegrees, EPS)
        assertEquals(0f, state.quietFade(), EPS)
        advanceUntilIdle()
        assertEquals(emptyList<DockletLanding>(), landings)
        assertEquals(emptyList<DockletMoment>(), moments)
    }

    @Test
    fun `an untuck shows a hidden dock at once with no landing or haptic`() = runTest {
        val state = hiddenState()

        state.untuck()
        advanceUntilIdle()

        assertTrue(state.isShownAtRest)
        assertEquals(emptyList<DockletLanding>(), landings)
        assertEquals(emptyList<DockletMoment>(), moments)
    }

    @Test
    fun `a tuck stops a restore in flight without reporting the dock shown`() = runTest {
        val state = hiddenState()
        state.restore()
        advanceTimeBy(FrameMillis * 6)
        runCurrent()

        state.tuck()
        advanceUntilIdle()

        assertTrue(state.isOffScreen)
        assertEquals(emptyList<DockletLanding>(), landings)
    }

    @Test
    fun `a tuck leaves a held dock in the hand`() = runTest {
        val state = hideState()
        state.startHide(DockletPoint(200f, 816f), time = 0.milliseconds)

        state.tuck()

        assertFalse(state.countsAsHidden)
        assertTrue(state.isHeld)
    }

    @Test
    fun `a restore from the tip or a key brings the dock back with a medium haptic`() = runTest {
        val state = hiddenState()

        state.restore()
        advanceUntilIdle()

        assertTrue(state.isShownAtRest)
        assertEquals(DockletLanding.Shown, landings.last())
        assertTrue(DockletMoment.Restored in moments)
    }

    @Test
    fun `a held dock moves with every finger move`() = runTest {
        val state = hideState()
        state.startHide(DockletPoint(50f, 816f), time = 0.milliseconds)
        val poses = mutableListOf<DockletPose>()
        val collector = backgroundScope.launch { snapshotFlow { state.pose() }.collect { poses += it } }
        runCurrent()

        state.moveHide(DockletPoint(150f, 816f), time = 16.milliseconds)
        Snapshot.sendApplyNotifications()
        runCurrent()
        collector.cancel()

        assertTrue("poses $poses", poses.size >= 2 && poses.last().angleDegrees > poses.first().angleDegrees)
    }

    @Test
    fun `a touch catches a dock coming back`() = runTest {
        val state = hiddenState()
        state.restore()
        advanceTimeBy(FrameMillis * 6)
        runCurrent()
        val before = state.pose()

        state.startHide(bridge.map(DockletPoint(200f, 816f), before), time = 1_000.milliseconds)
        runCurrent()

        assertEquals(before.angleDegrees, state.pose().angleDegrees, 0.5f)
    }

    @Test
    fun `a touch catches a dock hiding itself`() = runTest {
        val state = hideState()
        state.quietHide()
        advanceTimeBy(FrameMillis * 10)
        runCurrent()
        val before = state.pose()

        state.startHide(bridge.map(DockletPoint(200f, 816f), before), time = 1_000.milliseconds)
        runCurrent()

        assertTrue(before.angleDegrees > 0f)
        assertEquals(before.angleDegrees, state.pose().angleDegrees, 0.5f)
    }

    @Test
    fun `a short pull released near the edge leaves the dock hidden without a second landing`() = runTest {
        val state = hiddenState()

        state.startRestore(DockletPoint(395f, 500f), time = 0.milliseconds)
        state.moveRestore(DockletPoint(385f, 505f), time = 150.milliseconds)
        state.releaseRestore()
        advanceUntilIdle()

        assertTrue(state.countsAsHidden)
        assertEquals(emptyList<DockletLanding>(), landings)
        assertEquals(emptyList<DockletMoment>(), moments)
    }

    @Test
    fun `the quiet hide starts moving only from the second frame`() = runTest {
        val state = hideState()

        state.quietHide()
        advanceTimeBy(FrameMillis)
        runCurrent()

        assertEquals(0f, state.pose().angleDegrees, EPS)
    }

    @Test
    fun `the glow follows the peek out and back`() = runTest {
        val state = hiddenState()

        state.peek()
        advanceTimeBy(700L + 200L)
        runCurrent()

        assertTrue(state.glowAlpha < 1f)
    }

    @Test
    fun `a hide caught mid-flight and released past half still lands with the heavy haptic`() = runTest {
        val state = hideState()
        swipe(state, fromX = 50f, toX = 250f)
        state.releaseHide()
        advanceTimeBy(FrameMillis * 6)
        runCurrent()

        val caught = bridge.map(DockletPoint(200f, 816f), state.pose())
        state.startHide(caught, time = 1_000.milliseconds)
        state.moveHide(DockletPoint(caught.x + 40f, caught.y), time = 1_016.milliseconds)
        state.releaseHide()
        advanceUntilIdle()

        assertTrue(state.countsAsHidden)
        assertEquals(listOf(DockletLanding.HiddenBySwipe), landings)
        assertEquals(1, moments.count { it == DockletMoment.Hidden })
    }

    @Test
    fun `a restore caught mid-flight and released low still lands shown`() = runTest {
        val state = hiddenState()
        state.restore()
        advanceTimeBy(FrameMillis * 6)
        runCurrent()

        state.startRestore(DockletPoint(395f, 700f), time = 1_000.milliseconds)
        listOf(1_100L to DockletPoint(250f, 860f), 1_200L to DockletPoint(150f, 865f))
            .forEach { (time, point) -> state.moveRestore(point, time.milliseconds) }
        state.releaseRestore()
        advanceUntilIdle()

        assertTrue(state.isShownAtRest)
        assertEquals(listOf(DockletLanding.Shown), landings)
        assertEquals(1, moments.count { it == DockletMoment.Restored })
    }

    @Test
    fun `a restore before a hide lands still reports the dock shown`() = runTest {
        val state = hideState()
        swipe(state, fromX = 50f, toX = 250f)
        state.releaseHide()
        advanceTimeBy(FrameMillis * 3)
        runCurrent()

        state.restore()
        advanceUntilIdle()

        assertTrue(state.isShownAtRest)
        assertEquals(listOf(DockletLanding.Shown), landings)
        assertEquals(1, moments.count { it == DockletMoment.Restored })
    }

    @Test
    fun `a restore caught before it lands and pushed back past half reports the dock hidden`() = runTest {
        val state = hiddenState()
        state.restore()
        advanceTimeBy(FrameMillis * 3)
        runCurrent()

        val caught = bridge.map(DockletPoint(200f, 816f), state.pose())
        state.startHide(caught, time = 1_000.milliseconds)
        state.moveHide(DockletPoint(caught.x + 120f, caught.y), time = 1_016.milliseconds)
        state.releaseHide()
        advanceUntilIdle()

        assertTrue(state.countsAsHidden)
        assertEquals(listOf(DockletLanding.HiddenBySwipe), landings)
    }

    @Test
    fun `the dock is off screen only while hidden at rest and not peeking`() = runTest {
        val state = hideState()
        assertFalse(state.isOffScreen)

        swipe(state, fromX = 50f, toX = 250f)
        state.releaseHide()
        assertFalse(state.isOffScreen)
        advanceUntilIdle()
        assertTrue(state.isOffScreen)

        state.peek()
        advanceTimeBy(700L + 200L)
        runCurrent()
        assertFalse(state.isOffScreen)
        advanceUntilIdle()
        assertTrue(state.isOffScreen)
    }

    @Test
    fun `the hidden fraction follows the dock out and back`() = runTest {
        val state = hideState()
        assertEquals(0f, state.hiddenFraction, EPS)

        swipe(state, fromX = 50f, toX = 250f)
        state.releaseHide()
        advanceTimeBy(FrameMillis * 6)
        runCurrent()
        val midway = state.hiddenFraction
        advanceUntilIdle()
        val hidden = state.hiddenFraction
        state.restore()
        advanceUntilIdle()

        assertTrue(midway > 0f && midway < 1f)
        assertEquals(1f, hidden, EPS)
        assertEquals(0f, state.hiddenFraction, EPS)
    }

    @Test
    fun `the hidden fraction dips while a hidden dock peeks`() = runTest {
        val state = hiddenState()

        state.peek()
        advanceTimeBy(700L + 200L)
        runCurrent()

        assertTrue(state.hiddenFraction < 1f)
    }

    @Test
    fun `the hidden fraction stays within 0 and 1 through a flung hide`() = runTest {
        val state = hideState()
        swipe(state, fromX = 50f, toX = 300f, stepMillis = 4L)
        state.releaseHide()

        val fractions = (1..60).map {
            advanceTimeBy(FrameMillis)
            runCurrent()
            state.hiddenFraction
        }

        assertTrue(fractions.all { it in 0f..1f })
    }

    @Test
    fun `with reduced motion the hidden fraction follows the fade both ways`() = runTest {
        val state = hideState(DockletMotionMode.CrossFade)
        swipe(state, fromX = 50f, toX = 250f)
        state.releaseHide()
        advanceTimeBy(HalfFadeMillis)
        runCurrent()
        val fadingOut = state.hiddenFraction
        advanceUntilIdle()
        val hidden = state.hiddenFraction

        state.restore()
        advanceTimeBy(HalfFadeMillis)
        runCurrent()
        val fadingIn = state.hiddenFraction
        advanceUntilIdle()

        assertTrue(fadingOut > 0f && fadingOut < 1f)
        assertEquals(1f, hidden, EPS)
        assertTrue(fadingIn > 0f && fadingIn < 1f)
        assertEquals(0f, state.hiddenFraction, EPS)
    }

    private fun TestScope.hideState(motionMode: DockletMotionMode = DockletMotionMode.Morph) = DockletHideState(
        scope = CoroutineScope(coroutineContext + TestMonotonicFrameClock(this) + Job()),
        haptics = { moments += it },
        motionMode = { motionMode },
        currentDrawbridge = { bridge },
        onLanded = { landings += it },
    )

    private fun TestScope.hiddenState(): DockletHideState {
        val state = hideState()
        swipe(state, fromX = 50f, toX = 300f)
        state.releaseHide()
        advanceUntilIdle()
        landings.clear()
        moments.clear()
        return state
    }

    private fun swipe(state: DockletHideState, fromX: Float, toX: Float, stepMillis: Long = 16L) {
        state.startHide(DockletPoint(fromX, 816f), time = 0.milliseconds)
        val steps = 10
        for (step in 1..steps) {
            val x = fromX + (toX - fromX) * step / steps
            state.moveHide(DockletPoint(x, 816f), time = (step * stepMillis).milliseconds)
        }
    }

    private companion object {
        const val EPS = 1e-2f
        const val FrameMillis = 16L
        const val HalfFadeMillis = 110L
    }
}
