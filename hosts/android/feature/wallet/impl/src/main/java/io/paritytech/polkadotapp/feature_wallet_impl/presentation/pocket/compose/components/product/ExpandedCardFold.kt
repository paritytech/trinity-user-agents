package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import androidx.compose.animation.core.animate
import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.MutatorMutex
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * How much of the expanded card is folded away above the product.
 *
 * [foldedPx] runs from 0, the card fully shown, to [maxFoldPx], the card entirely out of the way and
 * the product holding every pixel below the top bar.
 */
@Stable
internal class ExpandedCardFoldState(initialFoldPx: Float = 0f) {
    var foldedPx by mutableFloatStateOf(initialFoldPx)
        private set

    var maxFoldPx by mutableFloatStateOf(0f)

    private var dragging = false
    private val animations = MutatorMutex()

    /** [delta] is a drag in screen terms, so dragging up — a negative delta — folds the card away. */
    fun drag(delta: Float) {
        foldedPx = (foldedPx - delta).coerceIn(0f, maxFoldPx)
    }

    /** Where a release from the current position lands. */
    fun settledTarget(): Float = if (foldedPx > maxFoldPx / 2f) maxFoldPx else 0f

    /** Stops any fold the page started: a drag outranks it. */
    suspend fun beginDrag() {
        dragging = true
        animations.mutate(MutatePriority.UserInput) {}
    }

    /**
     * The user owns the face until the settle ends, so the flag is cleared only on completion: a settle
     * cancelled by a new drag must leave the flag that drag set.
     */
    suspend fun endDrag() {
        animateTo(settledTarget(), MutatePriority.UserInput)
        dragging = false
    }

    /**
     * Folds the card away or back for the page. The answer comes as the fold starts, not when it ends,
     * so the page is not kept waiting on an animation.
     */
    suspend fun showFace(shown: Boolean): FaceShownAnswer {
        if (dragging) return FaceShownAnswer.USER_MOVING

        snapshotFlow { maxFoldPx }.first { it > 0f }
        if (dragging) return FaceShownAnswer.USER_MOVING

        CoroutineScope(currentCoroutineContext()).launch {
            animateTo(if (shown) 0f else maxFoldPx, MutatePriority.Default)
        }
        return FaceShownAnswer.APPLIED
    }

    private suspend fun animateTo(target: Float, priority: MutatePriority) {
        animations.mutate(priority) {
            if (dragging && priority == MutatePriority.Default) return@mutate
            if (foldedPx != target) {
                animate(initialValue = foldedPx, targetValue = target) { value, _ -> foldedPx = value }
            }
        }
    }

    companion object {
        val Saver: Saver<ExpandedCardFoldState, Float> = Saver(
            save = { it.foldedPx },
            restore = { ExpandedCardFoldState(it) }
        )
    }
}

@Composable
internal fun rememberExpandedCardFoldState(): ExpandedCardFoldState =
    rememberSaveable(saver = ExpandedCardFoldState.Saver) { ExpandedCardFoldState() }
