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
import androidx.compose.runtime.remember
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
internal class CardTurnState {
    var angle by mutableFloatStateOf(0f)
        private set

    val shownSide: PocketCardSide
        get() = if (cos(Math.toRadians(angle.toDouble())) >= 0) PocketCardSide.FRONT else PocketCardSide.BACK

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
        animations.mutate(MutatePriority.UserInput) {}
    }

    fun drag(deltaPx: Float, widthPx: Float) {
        if (!homing) angle += deltaPx * HALF_TURN / widthPx
    }

    /** Lands on the face the release heads for, at most [MAX_HALF_TURNS] half turns from the face then up. */
    suspend fun release(velocityPxPerSec: Float, widthPx: Float) {
        if (homing) return
        val velocity = velocityPxPerSec * HALF_TURN / widthPx
        val faceUp = nearest(angle, HALF_TURN)
        val reach = MAX_HALF_TURNS * HALF_TURN
        val rest = nearest(angle + velocity * FLING_PROJECTION_SECONDS, HALF_TURN).coerceIn(faceUp - reach, faceUp + reach)
        settle(rest, velocity, MutatePriority.Default)
        // Cleared only once the settle ends: one cut short by a new drag leaves the flag that drag set.
        dragging = false
    }

    suspend fun turnTo(side: PocketCardSide) {
        if (side != shownSide) settle(nearest(angle, HALF_TURN) + HALF_TURN, 0f, MutatePriority.Default)
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
        try {
            settle(nearest(angle, FULL_TURN), 0f, MutatePriority.PreventUserInput)
        } finally {
            homing = false
        }
    }

    /** Shows the front at once, stopping any turn under way. */
    suspend fun snapHome() = animations.mutate(MutatePriority.PreventUserInput) { angle = nearest(angle, FULL_TURN) }

    private suspend fun settle(target: Float, velocity: Float, priority: MutatePriority) = animations.mutate(priority) {
        animate(angle, target, velocity, spring(SETTLE_DAMPING, Spring.StiffnessMediumLow)) { value, _ -> angle = value }
    }

    private fun nearest(degrees: Float, step: Float) = (degrees / step).roundToInt() * step

    private companion object {
        const val HALF_TURN = 180f
        const val FULL_TURN = 360f
        const val FLING_PROJECTION_SECONDS = 0.3f
        const val MAX_HALF_TURNS = 4
        const val SETTLE_DAMPING = 0.85f
    }
}

@Composable
internal fun rememberCardTurnState(): CardTurnState = remember { CardTurnState() }
