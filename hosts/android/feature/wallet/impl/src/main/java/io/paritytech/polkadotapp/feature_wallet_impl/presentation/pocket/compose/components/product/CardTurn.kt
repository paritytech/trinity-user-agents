package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.spring
import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.MutatorMutex
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.launch
import uniffi.truapi.PocketCardSide
import kotlin.math.cos
import kotlin.math.roundToInt

/** How far the opened card is turned over. [angle] is in degrees: 0 is the front, 180 the back. */
@Stable
internal class CardTurnState(initialAngle: Float = 0f) {
    var angle by mutableFloatStateOf(initialAngle)
        private set

    val shownSide: PocketCardSide
        get() = sideAt(angle)

    /** Where the card is going: the end of a turn under way, else where it is. */
    private var heading = initialAngle

    /** The face up when the current drag began: a swipe turns the card at most one flip from it. */
    private var dragFace = initialAngle
    private var dragging = false
    private var homing = false
    private val animations = MutatorMutex()

    /** The rotation [side] is drawn at, kept within ±180° so the side up is never drawn mirrored. */
    fun layerRotation(side: PocketCardSide): Float {
        val turned = if (side == PocketCardSide.FRONT) angle else angle + HALF_TURN
        return (turned + HALF_TURN).mod(FULL_TURN) - HALF_TURN
    }

    /** Stops a settle under way: the finger outranks it. A card turning home is not stopped. */
    suspend fun beginDrag() {
        if (homing) return
        dragging = true
        dragFace = nearest(heading, HALF_TURN)
        animations.mutate(MutatePriority.UserInput) {}
    }

    fun drag(deltaPx: Float, widthPx: Float) {
        if (homing) return
        angle = (angle + deltaPx * HALF_TURN / widthPx).withinOneFlip()
        heading = angle
    }

    /** Lands on the face the release heads for, never more than one flip from the face the drag began on. */
    suspend fun release(velocityPxPerSec: Float, widthPx: Float) {
        if (homing) return
        // Capped so a hard flick still settles on the next face instead of swinging past it.
        val velocity = (velocityPxPerSec * HALF_TURN / widthPx).coerceIn(-MAX_SETTLE_SPEED, MAX_SETTLE_SPEED)
        val rest = nearest(angle + velocity * FLING_PROJECTION_SECONDS, HALF_TURN).withinOneFlip()
        settle(rest, velocity, MutatePriority.UserInput)
        // Cleared only once the settle ends: one cut short by a new drag leaves the flag that drag set.
        dragging = false
    }

    suspend fun turnTo(side: PocketCardSide) {
        if (side != sideAt(heading)) turnOver()
    }

    /** Turns to the other side of where the card is going, so it reverses a turn under way. */
    suspend fun turnOver() {
        // Set now, not once the lock is taken, so a request queued behind this one sees where it goes.
        heading = nearest(heading, HALF_TURN) + HALF_TURN
        settle(heading, 0f, MutatePriority.Default)
    }

    /**
     * Turns the card for its page, answering as the turn starts. The user's drag wins, and asking for
     * the front of a card without a back succeeds.
     */
    suspend fun requestSide(side: PocketCardSide, hasBack: Boolean): FaceShownAnswer {
        if (side == PocketCardSide.BACK && !hasBack) return FaceShownAnswer.NO_BACK
        if (dragging) return FaceShownAnswer.USER_MOVING

        CoroutineScope(currentCoroutineContext()).launch { turnTo(side) }
        return FaceShownAnswer.APPLIED
    }

    /** Turns the card back to its front for closing, which a drag cannot interrupt. */
    suspend fun turnHome() {
        homing = true
        dragging = false
        try {
            settle(nearest(angle, FULL_TURN), 0f, MutatePriority.PreventUserInput)
        } finally {
            homing = false
        }
    }

    /** Shows the front at once, stopping any turn under way. */
    suspend fun snapHome() = animations.mutate(MutatePriority.PreventUserInput) {
        dragging = false
        angle = nearest(angle, FULL_TURN)
        heading = angle
    }

    private suspend fun settle(target: Float, velocity: Float, priority: MutatePriority) = animations.mutate(priority) {
        // A page's turn queued before a finger landed must not move the card under it.
        if (dragging && priority == MutatePriority.Default) return@mutate
        heading = target
        animate(angle, target, velocity, spring(SETTLE_DAMPING, Spring.StiffnessMediumLow)) { value, _ -> angle = value }
    }

    private fun sideAt(degrees: Float) =
        if (cos(Math.toRadians(degrees.toDouble())) >= 0) PocketCardSide.FRONT else PocketCardSide.BACK

    private fun nearest(degrees: Float, step: Float) = (degrees / step).roundToInt() * step

    private fun Float.withinOneFlip() = coerceIn(dragFace - HALF_TURN, dragFace + HALF_TURN)

    companion object {
        private const val HALF_TURN = 180f
        private const val FULL_TURN = 360f
        private const val FLING_PROJECTION_SECONDS = 0.3f
        private const val MAX_SETTLE_SPEED = 720f
        private const val SETTLE_DAMPING = 0.85f

        val Saver: Saver<CardTurnState, Float> =
            Saver(save = { it.nearest(it.heading, HALF_TURN) }, restore = { CardTurnState(it) })
    }
}

@Composable
internal fun rememberCardTurnState(): CardTurnState = rememberSaveable(saver = CardTurnState.Saver) { CardTurnState() }
