package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components

import androidx.compose.runtime.MonotonicFrameClock
import androidx.compose.ui.unit.dp
import io.mockk.mockk
import io.mockk.verify
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds

private val FRAME = 16.milliseconds

private class SteppingFrameClock : MonotonicFrameClock {
    private var nanos = 0L

    override suspend fun <R> withFrameNanos(onFrame: (Long) -> R): R {
        delay(FRAME)
        nanos += FRAME.inWholeNanoseconds
        return onFrame(nanos)
    }
}

private const val WINDOW_HEIGHT = 4000f

@OptIn(ExperimentalCoroutinesApi::class)
class CardCloseDragStateTest {
    private val state = CardCloseDragState(WINDOW_HEIGHT)
    private val onClose = mockk<() -> Unit>(relaxed = true)

    // The card travels back into the list from where the finger let go, so the offset stays put.
    @Test
    fun `release past the close distance closes once`() = runTest(SteppingFrameClock()) {
        state.drag(1001f)

        state.release(onClose)
        advanceUntilIdle()

        verifyClosedOnce()
        assertEquals(1001f, state.offsetPx, 0f)
    }

    @Test
    fun `release exactly at the close distance does not close`() = runTest(SteppingFrameClock()) {
        state.drag(1000f)

        state.release(onClose)
        advanceUntilIdle()

        verifyNotClosed()
        assertEquals(0f, state.offsetPx, 0f)
    }

    @Test
    fun `close distance is a quarter of the window height and never below one pixel`() {
        assertEquals(585f, CardCloseDragState(2340f).closeDistancePx, 0f)
        assertEquals(400f, CardCloseDragState(1600f).closeDistancePx, 0f)
        assertEquals(1f, CardCloseDragState(0f).closeDistancePx, 0f)
        assertEquals(1f, CardCloseDragState(2f).closeDistancePx, 0f)
    }

    @Test
    fun `the card follows the finger down and never rises above rest`() {
        state.drag(-50f)
        assertEquals(0f, state.offsetPx, 0f)

        state.drag(200f)
        state.drag(-500f)
        assertEquals(0f, state.offsetPx, 0f)

        state.drag(200f)
        state.drag(-80f)
        assertEquals(120f, state.offsetPx, 0f)
    }

    @Test
    fun `progress grows with the drag and caps at one`() {
        assertEquals(0f, state.progress, 0f)

        state.drag(500f)
        assertEquals(0.5f, state.progress, 0f)

        state.drag(500f)
        assertEquals(1f, state.progress, 0f)

        state.drag(2000f)
        assertEquals(1f, state.progress, 0f)
    }

    @Test
    fun `blur grows with the drag up to 12 dp`() {
        assertEquals(0.dp, state.blurSigma)

        state.drag(500f)
        assertEquals(6.dp, state.blurSigma)

        state.drag(500f)
        assertEquals(12.dp, state.blurSigma)

        state.drag(2000f)
        assertEquals(12.dp, state.blurSigma)
    }

    @Test
    fun `release short of the close distance springs back without closing`() = runTest(SteppingFrameClock()) {
        state.drag(400f)

        state.release(onClose)
        advanceUntilIdle()

        verifyNotClosed()
        assertEquals(0f, state.offsetPx, 0f)
    }

    // A finger landing on the card mid-spring must hold it there, or the spring pulls it away from the finger.
    @Test
    fun `a drag started during the spring back takes over`() = runTest(SteppingFrameClock()) {
        state.drag(400f)
        launch { state.release(onClose) }
        advanceTimeBy(FRAME * 3)
        val midSpring = state.offsetPx
        assertTrue(midSpring > 0f && midSpring < 400f)

        state.beginDrag()
        advanceUntilIdle()

        assertEquals(midSpring, state.offsetPx, 0f)
        verifyNotClosed()
    }

    @Test
    fun `reset puts a closed card back at rest`() = runTest(SteppingFrameClock()) {
        state.drag(1001f)
        state.release(onClose)

        state.reset()

        assertEquals(0f, state.offsetPx, 0f)
        assertEquals(0f, state.progress, 0f)
    }

    @Test
    fun `an unmeasured window leaves the content fully shown`() {
        val unmeasured = CardCloseDragState(0f)

        assertEquals(0f, unmeasured.progress, 0f)
        assertEquals(0.dp, unmeasured.blurSigma)
    }

    @Test
    fun `a drag on the product card closes only from its unfolded rest`() {
        assertTrue("down from rest", state.takesDrag(delta = 10f, foldedPx = 0f))
        assertFalse("up from rest folds", state.takesDrag(delta = -10f, foldedPx = 0f))
        assertFalse("down on a folded card unfolds", state.takesDrag(delta = 10f, foldedPx = 40f))

        state.drag(100f)
        assertTrue("up while pulled down", state.takesDrag(delta = -10f, foldedPx = 0f))
    }

    private fun verifyClosedOnce() = verify(exactly = 1) { onClose() }

    private fun verifyNotClosed() = verify(exactly = 0) { onClose() }
}
