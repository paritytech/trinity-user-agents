package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.TestMonotonicFrameClock
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class, ExperimentalTestApi::class)
class DockletCardStateTest {
    @Test
    fun `drag is ignored when the card closed during a swipe down`() = runTest {
        val state = openCard(DockletStep.Step1)
        state.startSwipeDown()
        state.dragSwipe(0.8f)

        state.close()
        state.dragSwipe(0.9f)
        runCurrent()

        assertEquals(0.8f, state.progress, EPS)
    }

    @Test
    fun `swipe-down release is ignored when the card closed during the drag`() = runTest {
        val state = openCard(DockletStep.Step1)
        state.startSwipeDown()
        state.dragSwipe(0.9f)
        var closeRequested = false

        state.close()
        state.releaseSwipeDown(closedFraction = 0.1f, downwardDpPerSecond = 0f, travel = Travel) { closeRequested = true }
        advanceUntilIdle()

        assertFalse(closeRequested)
        assertEquals(0f, state.progress, EPS)
    }

    @Test
    fun `swipe-up release is ignored when the card closed during the drag`() = runTest {
        val state = cardState()
        state.startSwipeUp()
        state.dragSwipe(0.6f)
        var openRequested = false

        state.close()
        state.releaseSwipeUp(0.6f, 0f, Travel, onOpened = { openRequested = true }, onKeyboardRequest = {})
        advanceUntilIdle()

        assertFalse(openRequested)
        assertEquals(0f, state.progress, EPS)
    }

    @Test
    fun `swipe down does not start when the card is closed`() = runTest {
        val state = cardState()

        state.startSwipeDown()

        assertFalse(state.swipeDriven)
    }

    @Test
    fun `swipe down ends the drag when it springs back`() = runTest {
        val state = openCard(DockletStep.Step1)
        state.startSwipeDown()
        state.dragSwipe(0.9f)

        state.releaseSwipeDown(closedFraction = 0.1f, downwardDpPerSecond = 0f, travel = Travel) {}
        advanceUntilIdle()

        assertFalse(state.swipeDriven)
        assertEquals(1f, state.progress, EPS)
    }

    @Test
    fun `swipe down closes the card when released past half`() = runTest {
        val state = openCard(DockletStep.Step1)
        state.startSwipeDown()
        state.dragSwipe(0.4f)
        var closeRequested = false

        state.releaseSwipeDown(closedFraction = 0.6f, downwardDpPerSecond = 0f, travel = Travel) { closeRequested = true }
        advanceUntilIdle()

        assertTrue(closeRequested)
        assertEquals(0f, state.progress, EPS)
    }

    @Test
    fun `swipe down ends the drag when it closes`() = runTest {
        val state = openCard(DockletStep.Step1)
        state.startSwipeDown()
        state.dragSwipe(0.4f)

        state.releaseSwipeDown(closedFraction = 0.6f, downwardDpPerSecond = 0f, travel = Travel) {}
        advanceUntilIdle()

        assertFalse(state.swipeDriven)
    }

    @Test
    fun `step frame moves from the shape on screen when the card reopens during a close`() = runTest {
        val state = openCard(DockletStep.Step2)
        state.close()
        advanceTimeBy(FrameMillis * 3)

        state.open(DockletStep.Step1)
        advanceTimeBy(FrameMillis)
        runCurrent()

        assertTrue("step progress ${state.stepProgress}", state.stepProgress > 0.9f)
    }

    @Test
    fun `swipe up follows the finger when the drag starts at rest`() = runTest {
        val state = cardState()

        state.startSwipeUp()
        state.dragSwipe(0.6f)

        assertTrue(state.swipeDriven)
        assertEquals(0.6f, state.progress, EPS)
    }

    @Test
    fun `swipe up opens the card when released past half`() = runTest {
        val state = cardState()
        state.startSwipeUp()
        state.dragSwipe(0.6f)
        var openRequests = 0

        state.releaseSwipeUp(0.6f, 0f, Travel, onOpened = { openRequests++ }, onKeyboardRequest = {})
        advanceUntilIdle()

        assertEquals(1, openRequests)
        assertEquals(1f, state.progress, EPS)
    }

    @Test
    fun `swipe up ends the drag when released past half`() = runTest {
        val state = cardState()
        state.startSwipeUp()
        state.dragSwipe(0.6f)

        state.releaseSwipeUp(0.6f, 0f, Travel, onOpened = {}, onKeyboardRequest = {})
        advanceUntilIdle()

        assertFalse(state.swipeDriven)
    }

    @Test
    fun `swipe up returns to rest and ends the drag when released below half`() = runTest {
        val state = cardState()
        state.startSwipeUp()
        state.dragSwipe(0.3f)

        state.releaseSwipeUp(0.3f, 0f, Travel, onOpened = {}, onKeyboardRequest = {})
        advanceUntilIdle()

        assertFalse(state.swipeDriven)
        assertEquals(0f, state.progress, EPS)
    }

    @Test
    fun `second swipe-up release is ignored when the first already resolved the drag`() = runTest {
        val state = cardState()
        state.startSwipeUp()
        state.dragSwipe(0.6f)
        var openRequests = 0

        state.releaseSwipeUp(0.6f, 0f, Travel, onOpened = { openRequests++ }, onKeyboardRequest = {})
        state.releaseSwipeUp(0.6f, 0f, Travel, onOpened = { openRequests++ }, onKeyboardRequest = {})
        advanceUntilIdle()

        assertEquals(1, openRequests)
    }

    @Test
    fun `open ends the drag when the card opens during a swipe-up return to rest`() = runTest {
        val state = cardState()
        state.startSwipeUp()
        state.dragSwipe(0.3f)
        state.releaseSwipeUp(0.3f, 0f, Travel, onOpened = {}, onKeyboardRequest = {})
        advanceTimeBy(FrameMillis * 2)

        state.open(DockletStep.Step1)

        assertFalse(state.swipeDriven)
    }

    @Test
    fun `cross-fade starts from rest when animations turned off after a morph swipe`() = runTest {
        var mode = DockletMotionMode.Morph
        val state = cardState { mode }
        state.startSwipeUp()
        state.dragSwipe(0.6f)
        state.releaseSwipeUp(0.6f, 0f, Travel, onOpened = {}, onKeyboardRequest = {})
        advanceUntilIdle()
        state.close()
        advanceUntilIdle()

        mode = DockletMotionMode.CrossFade
        state.open(DockletStep.Step1)
        runCurrent()

        assertEquals(0f, state.crossFadeProgress, EPS)
    }

    @Test
    fun `swipe up cross-fades when animations turned off before the swipe`() = runTest {
        var mode = DockletMotionMode.Morph
        val state = cardState { mode }

        mode = DockletMotionMode.CrossFade
        state.startSwipeUp()

        assertEquals(DockletMotionMode.CrossFade, state.motionMode)
    }

    @Test
    fun `open is ignored when the card is already open`() = runTest {
        val state = cardState()
        state.startSwipeUp()
        state.dragSwipe(0.6f)

        state.releaseSwipeUp(0.6f, 0f, Travel, onOpened = { state.open(DockletStep.Step1) }, onKeyboardRequest = {})
        advanceUntilIdle()

        assertEquals(DockletStep.Step2, state.step)
    }

    @Test
    fun `close is ignored when a swipe down already closed the card`() = runTest {
        val moments = mutableListOf<DockletMoment>()
        val state = cardState(haptics = { moments += it })
        state.open(DockletStep.Step1)
        advanceUntilIdle()
        state.startSwipeDown()
        state.dragSwipe(0.4f)
        state.releaseSwipeDown(closedFraction = 0.6f, downwardDpPerSecond = 0f, travel = Travel) {}
        advanceTimeBy(FrameMillis * 2)

        state.close()

        assertEquals(1, moments.count { it == DockletMoment.Close })
    }

    private fun TestScope.cardState(
        haptics: DockletHaptics = DockletHaptics {},
        motionMode: () -> DockletMotionMode = { DockletMotionMode.Morph },
    ) = DockletCardState(
        scope = CoroutineScope(coroutineContext + TestMonotonicFrameClock(this) + Job()),
        haptics = haptics,
        currentMotionMode = motionMode,
    )

    private fun TestScope.openCard(step: DockletStep): DockletCardState {
        val state = cardState()
        state.open(step)
        advanceUntilIdle()
        return state
    }

    private companion object {
        const val EPS = 1e-3f
        const val FrameMillis = 16L
        val Travel = 300.dp
    }
}
