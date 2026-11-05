package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer.APPLIED
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer.NO_BACK
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer.USER_MOVING
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
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
    private fun dragged(px: Float) = CardTurnState().apply { drag(px, CARD_WIDTH) }

    /** Each frame's angle, and the rotation the face then up is drawn at, until [motion] ends. */
    private fun TestScope.framesOf(state: CardTurnState, motion: suspend CoroutineScope.() -> Unit): List<Pair<Float, Float>> {
        val running = async { motion() }
        val frames = mutableListOf<Pair<Float, Float>>()
        while (!running.isCompleted) {
            advanceTimeBy(FRAME_MILLIS)
            runCurrent()
            frames += state.angle to state.layerRotation(state.shownSide)
        }
        return frames
    }

    // A card let go part way must land on a face; a short accidental drag must not turn it over.
    @Test
    fun `a card released at rest turns over only when dragged past half its width`() = runTest(SteppingFrameClock) {
        val short = dragged(CARD_WIDTH / 2 - 10)
        val long = dragged(CARD_WIDTH / 2 + 10)

        short.release(0f, CARD_WIDTH)
        long.release(0f, CARD_WIDTH)

        assertEquals(listOf(0f, 180f), listOf(short.angle, long.angle))
        assertEquals(listOf(PocketCardSide.FRONT, PocketCardSide.BACK), listOf(short.shownSide, long.shownSide))
    }

    // The flick in the video spins the card several times and lands on a face, never mirrored on the way.
    @Test
    fun `a flick keeps turning its way, lands on a face no more than four half turns on, and never shows a face mirrored`() =
        runTest(SteppingFrameClock) {
            for ((start, velocity) in listOf(0f to 20_000f, 0f to -20_000f, 100f to 20_000f)) {
                val state = dragged(start / 180f * CARD_WIDTH)
                val faceAtRelease = 180f * Math.round(state.angle / 180f)

                val frames = framesOf(state) { state.release(velocity, CARD_WIDTH) }

                val rest = frames.last().first
                assertEquals("lands on a face from $start", 0f, rest.mod(180f), 0f)
                assertTrue("turns at most four half turns from $start", abs(rest - faceAtRelease) <= 4 * 180f)
                assertTrue("keeps turning its way from $start", (frames[1].first - start) * velocity > 0)
                assertTrue("the face up is never mirrored", frames.all { (_, upRotation) -> abs(upRotation) <= 90f })
            }
        }

    @Test
    fun `turning to a side shows that side, and turning to the side already up does nothing`() = runTest(SteppingFrameClock) {
        val state = CardTurnState()

        state.turnTo(PocketCardSide.FRONT)
        val unmoved = state.angle
        state.turnTo(PocketCardSide.BACK)
        val turned = state.angle
        state.turnTo(PocketCardSide.FRONT)

        assertEquals(listOf(0f, 180f), listOf(unmoved, turned))
        assertEquals(0f, state.angle.mod(360f), 0f)
    }

    // A card leaving shrinks back into the list, which shows only fronts.
    @Test
    fun `closing turns the card home, even mid-settle and with a drag on it`() = runTest(SteppingFrameClock) {
        for (start in listOf(540f, 200f)) {
            val state = dragged(start / 180f * CARD_WIDTH)
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

    // A page asking while the user turns the card would fight the finger, so it is told the user is moving.
    @Test
    fun `the page turns the card, unless there is no back or the user is turning it`() = runTest(SteppingFrameClock) {
        val state = CardTurnState()
        val answers = mutableListOf(
            state.requestSide(PocketCardSide.BACK, hasBack = false),
            state.requestSide(PocketCardSide.FRONT, hasBack = false),
            state.requestSide(PocketCardSide.BACK, hasBack = true),
        )
        advanceUntilIdle()
        val turned = state.angle

        state.beginDrag()
        answers += state.requestSide(PocketCardSide.FRONT, hasBack = true)
        advanceUntilIdle()

        assertEquals(listOf(NO_BACK, APPLIED, APPLIED, USER_MOVING), answers)
        assertEquals(listOf(180f, 180f), listOf(turned, state.angle))
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
