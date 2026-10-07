package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import androidx.compose.runtime.MonotonicFrameClock
import androidx.compose.runtime.snapshots.Snapshot
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

private const val FRAME_NANOS = 16_000_000L

private object SteppingFrameClock : MonotonicFrameClock {
    private var nanos = 0L

    override suspend fun <R> withFrameNanos(onFrame: (Long) -> R): R {
        delay(FRAME_NANOS / 1_000_000)
        nanos += FRAME_NANOS
        return onFrame(nanos)
    }
}

private const val CARD_HEIGHT = 250f

@OptIn(ExperimentalCoroutinesApi::class)
class ExpandedCardFoldStateTest {
    private val state = ExpandedCardFoldState().apply { maxFoldPx = CARD_HEIGHT }

    @Test
    fun `dragging up folds the card away by the distance dragged`() {
        state.drag(-100f)

        assertEquals(100f, state.foldedPx, 0f)
    }

    // The product below grows by exactly what the card gives up, so a fold past the card's own
    // height would leave the product taller than the space there is.
    @Test
    fun `the card folds no further than its own height`() {
        state.drag(-400f)

        assertEquals(CARD_HEIGHT, state.foldedPx, 0f)
    }

    // The card is the default, so dragging down from it must not pull the product off the bottom.
    @Test
    fun `the card cannot be dragged further open than shown`() {
        state.drag(200f)

        assertEquals(0f, state.foldedPx, 0f)
    }

    @Test
    fun `dragging back down unfolds the card again`() {
        state.drag(-200f)
        state.drag(120f)

        assertEquals(80f, state.foldedPx, 0f)
    }

    /**
     * Released mid-drag the card has to pick a side: left part-folded it would crop the face and
     * leave the product an arbitrary height that neither side designed for.
     */
    @Test
    fun `released past halfway the card settles out of the way`() {
        state.drag(-(CARD_HEIGHT / 2f + 1f))

        assertEquals(CARD_HEIGHT, state.settledTarget(), 0f)
    }

    @Test
    fun `released before halfway the card settles back to shown`() {
        state.drag(-(CARD_HEIGHT / 2f - 1f))

        assertEquals(0f, state.settledTarget(), 0f)
    }

    /** The page asking for the card back or away is the point of the feature: the fold must follow it. */
    @Test
    fun `the page can fold the card away and back`() = runTest(SteppingFrameClock) {
        assertEquals(FaceShownAnswer.APPLIED, state.showFace(shown = false))
        advanceUntilIdle()
        assertEquals(CARD_HEIGHT, state.foldedPx, 0f)

        assertEquals(FaceShownAnswer.APPLIED, state.showFace(shown = true))
        advanceUntilIdle()
        assertEquals(0f, state.foldedPx, 0f)
    }

    // The user's finger decides while it is down; the page must be told so instead of fighting it.
    @Test
    fun `a request during a drag is answered user moving and changes nothing`() = runTest(SteppingFrameClock) {
        state.beginDrag()
        state.drag(-100f)

        assertEquals(FaceShownAnswer.USER_MOVING, state.showFace(shown = false))
        advanceUntilIdle()

        assertEquals(100f, state.foldedPx, 0f)
    }

    // A page can ask as soon as it loads, before the card has been laid out and its height is known.
    @Test
    fun `a request before the card is measured waits and then applies`() = runTest(SteppingFrameClock) {
        val unmeasured = ExpandedCardFoldState()
        val answer = async { unmeasured.showFace(shown = false) }
        runCurrent()
        assertEquals(false, answer.isCompleted)

        unmeasured.maxFoldPx = CARD_HEIGHT
        Snapshot.sendApplyNotifications()
        assertEquals(FaceShownAnswer.APPLIED, answer.await())
        advanceUntilIdle()

        assertEquals(CARD_HEIGHT, unmeasured.foldedPx, 0f)
    }

    // The user's drag always wins: a fold the page started must stop the moment a finger lands.
    @Test
    fun `a drag started during a page-driven fold takes over`() = runTest(SteppingFrameClock) {
        state.showFace(shown = false)
        advanceTimeBy(FRAME_NANOS / 1_000_000 * 3)
        val midFold = state.foldedPx
        assertEquals(true, midFold > 0f && midFold < CARD_HEIGHT)

        state.beginDrag()
        advanceUntilIdle()

        assertEquals(midFold, state.foldedPx, 0f)
    }
}
