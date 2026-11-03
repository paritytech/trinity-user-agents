package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.spring
import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.MutatorMutex
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

private const val CLOSE_DISTANCE_WINDOW_FRACTION = 0.25f
private const val MIN_CLOSE_DISTANCE_PX = 1f

private val CloseDragMaxBlur = 12.dp

// SwiftUI's .spring(duration: 0.45, bounce: 0.15), which the iOS card springs back with.
private val SpringBack = spring<Float>(dampingRatio = 0.85f, stiffness = Spring.StiffnessLow)

@Stable
internal class CardCloseDragState(windowHeightPx: Float) {
    val closeDistancePx = (windowHeightPx * CLOSE_DISTANCE_WINDOW_FRACTION).coerceAtLeast(MIN_CLOSE_DISTANCE_PX)

    var offsetPx by mutableFloatStateOf(0f)
        private set

    val progress: Float get() = (offsetPx / closeDistancePx).coerceAtMost(1f)

    val blurSigma: Dp get() = CloseDragMaxBlur * progress

    private val animations = MutatorMutex()

    fun drag(delta: Float) {
        offsetPx = (offsetPx + delta).coerceAtLeast(0f)
    }

    suspend fun beginDrag() {
        animations.mutate(MutatePriority.UserInput) {}
    }

    suspend fun release(onClose: () -> Unit) {
        if (offsetPx > closeDistancePx) {
            onClose()
        } else {
            animations.mutate {
                animate(initialValue = offsetPx, targetValue = 0f, animationSpec = SpringBack) { value, _ -> offsetPx = value }
            }
        }
    }
}
