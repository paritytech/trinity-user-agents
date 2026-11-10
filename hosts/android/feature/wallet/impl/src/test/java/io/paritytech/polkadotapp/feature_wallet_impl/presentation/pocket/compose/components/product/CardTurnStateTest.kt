package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer.APPLIED
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer.NO_BACK
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer.USER_MOVING
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownRequest
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.PocketCardSide
import kotlin.math.abs

private const val CARD_WIDTH = 300f
private const val FRAME_MILLIS = FRAME_NANOS / 1_000_000

@OptIn(ExperimentalCoroutinesApi::class)
class CardTurnStateTest {
    private suspend fun dragged(px: Float) = CardTurnState().apply {
        beginDrag()
        drag(px, CARD_WIDTH)
    }

    // A card let go part way must land on a face; a short accidental drag must not turn it over.
    @Test
    fun `a card released at rest turns over only when dragged past half its width`() = runTest(SteppingFrameClock) {
        val short = dragged(CARD_WIDTH / 2 - 10)
        val long = dragged(CARD_WIDTH / 2 + 10)

        short.release(0f, CARD_WIDTH)
        long.release(0f, CARD_WIDTH)

        assertEquals(listOf(PocketCardSide.FRONT, PocketCardSide.BACK), listOf(short.shownSide, long.shownSide))
    }

    // A swipe shows the other side of the card; it is not a spinner, however far or hard it goes.
    @Test
    fun `a swipe turns the card over once, however far or hard`() = runTest(SteppingFrameClock) {
        for ((draggedPx, velocity) in listOf(0f to 20_000f, 0f to -20_000f, 2 * CARD_WIDTH to 20_000f, -2 * CARD_WIDTH to 0f)) {
            val state = dragged(draggedPx)

            state.release(velocity, CARD_WIDTH)

            assertEquals("after $draggedPx at $velocity", 180f, abs(state.angle), 0f)
        }
    }

    // The page turns the card the way a right thumb swipes it, right to left, which lowers the angle.
    @Test
    fun `turning to a side shows that side, and turning to the side already up does nothing`() = runTest(SteppingFrameClock) {
        val state = CardTurnState()

        state.turnTo(PocketCardSide.FRONT)
        val unmoved = state.angle
        state.turnTo(PocketCardSide.BACK)
        val turned = state.angle
        state.turnTo(PocketCardSide.FRONT)

        assertEquals(listOf(0f, -180f), listOf(unmoved, turned))
        assertEquals(0f, state.angle.mod(360f), 0f)
    }

    // A card leaving shrinks back into the list, which shows only fronts.
    @Test
    fun `closing turns the card home, even mid-settle and with a drag on it`() = runTest(SteppingFrameClock) {
        for (start in listOf(540f, 200f)) {
            val state = CardTurnState(start)
            state.turnHome()
            assertEquals("from $start", 0f, state.angle.mod(360f), 0f)
        }

        val state = dragged(CARD_WIDTH * 0.8f)
        launch { state.release(5_000f, CARD_WIDTH) }
        advanceTimeBy(3 * FRAME_MILLIS)
        val home = launch { state.turnHome() }
        advanceTimeBy(FRAME_MILLIS)
        state.beginDrag()
        state.drag(CARD_WIDTH, CARD_WIDTH)
        advanceUntilIdle()

        assertTrue(home.isCompleted)
        assertEquals(0f, state.angle.mod(360f), 0f)
    }

    // A back that disappears while it shows leaves nothing to turn to, so the front comes back without a turn.
    @Test
    fun `snapping home lands at once, and a settle under way does not move it after`() = runTest(SteppingFrameClock) {
        val state = dragged(CARD_WIDTH * 0.8f)
        launch { state.release(5_000f, CARD_WIDTH) }
        advanceTimeBy(3 * FRAME_MILLIS)

        state.snapHome()
        val snapped = state.angle
        advanceUntilIdle()

        assertEquals(0f, snapped.mod(360f), 0f)
        assertEquals(snapped, state.angle, 0f)
    }

    @Test
    fun `the page turns the card, unless there is no back`() = runTest(SteppingFrameClock) {
        val turn = CardTurnState()
        val answers = listOf(true to false, false to false, true to true).map { (shown, hasBack) ->
            answer(FaceShownRequest(shown, CompletableDeferred(), back = true), ExpandedCardFoldState(), turn, hasBack)
        }
        advanceUntilIdle()

        assertEquals(listOf(NO_BACK, APPLIED, APPLIED) to -180f, answers to turn.angle)
    }

    // The user's finger on the card wins over the page, whether it is folding the face or turning the card. A missing
    // back is told at once, so the page does not wait for the finger only to retry into NoBack.
    @Test
    fun `a request while the user turns the card is answered UserMoving and moves nothing`() = runTest(SteppingFrameClock) {
        val fold = ExpandedCardFoldState().apply { maxFoldPx = 100f }
        val turn = dragged(CARD_WIDTH / 10)
        val held = turn.angle

        val answers = listOf(false to true, true to true, true to false).map { (back, hasBack) ->
            answer(FaceShownRequest(shown = true, reply = CompletableDeferred(), back = back), fold, turn, hasBack)
        }
        advanceUntilIdle()

        assertEquals(listOf(USER_MOVING, USER_MOVING, NO_BACK) to listOf(0f, held), answers to listOf(fold.foldedPx, turn.angle))
    }

    // The page is told its request applied, so the card has to end on the side it asked for last.
    @Test
    fun `the page's last request wins, even while the card is still turning`() = runTest(SteppingFrameClock) {
        val state = CardTurnState()

        state.requestSide(PocketCardSide.BACK)
        advanceTimeBy(4 * FRAME_MILLIS)
        state.requestSide(PocketCardSide.FRONT)
        advanceUntilIdle()
        val afterFront = state.angle
        state.requestSide(PocketCardSide.BACK)
        advanceTimeBy(4 * FRAME_MILLIS)
        state.requestSide(PocketCardSide.FRONT)
        state.requestSide(PocketCardSide.BACK)
        advanceUntilIdle()

        assertEquals(0f to PocketCardSide.BACK, afterFront to state.shownSide)
    }

    // The page is told its request applied, so a turn cut short with the request must still end when asked again.
    @Test
    fun `asking again for a side whose turn was cut short finishes the turn`() = runTest(SteppingFrameClock) {
        val state = CardTurnState()
        val request = launch { state.turnTo(PocketCardSide.BACK) }
        advanceTimeBy(4 * FRAME_MILLIS)
        request.cancel()

        state.turnTo(PocketCardSide.BACK)

        assertEquals(-180f, state.angle, 0f)
    }

    // A screen-reader user asks for the other side of where the card is going, not of where it is mid-turn.
    @Test
    fun `turning over mid-turn reverses the turn`() = runTest(SteppingFrameClock) {
        val state = CardTurnState()

        state.requestSide(PocketCardSide.BACK)
        advanceTimeBy(4 * FRAME_MILLIS)
        launch { state.turnOver() }
        advanceUntilIdle()

        assertEquals(0f, state.angle, 0f)
    }

    // A finger that lands just as the page asks must not be fought by the page's turn.
    @Test
    fun `a page turn gives way to a drag that starts before it runs`() = runTest(SteppingFrameClock) {
        val state = CardTurnState()

        state.requestSide(PocketCardSide.BACK)
        state.beginDrag()
        state.drag(CARD_WIDTH / 10, CARD_WIDTH)
        advanceUntilIdle()

        assertEquals(PocketCardSide.FRONT, state.shownSide)
    }

    // A fling's settle can end without the user, when the card goes home; the page must not hear UserMoving ever after.
    @Test
    fun `a fling cut short by the card going home leaves the card to the page`() = runTest(SteppingFrameClock) {
        val state = CardTurnState()
        state.beginDrag()
        state.drag(CARD_WIDTH * 0.8f, CARD_WIDTH)
        launch { state.release(5_000f, CARD_WIDTH) }
        advanceTimeBy(3 * FRAME_MILLIS)

        state.snapHome()

        assertEquals(APPLIED, state.requestSide(PocketCardSide.BACK))
    }

    @Test
    fun `a new drag stops the card settling`() = runTest(SteppingFrameClock) {
        val state = dragged(CARD_WIDTH * 0.8f)
        val settle = launch { state.release(0f, CARD_WIDTH) }
        advanceTimeBy(3 * FRAME_MILLIS)

        state.beginDrag()
        val held = state.angle
        advanceUntilIdle()

        assertTrue(settle.isCancelled)
        assertEquals(held, state.angle, 0f)
    }
}
