package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.spring
import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.MutatorMutex
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.isPocketScreenLeaving

private const val CLOSE_DISTANCE_WINDOW_FRACTION = 0.25f
private const val MIN_CLOSE_DISTANCE_PX = 1f
private const val SPRING_BACK_DAMPING_RATIO = 0.85f

private val CloseDragMaxBlur = 12.dp

private val SpringBack = spring<Float>(dampingRatio = SPRING_BACK_DAMPING_RATIO, stiffness = Spring.StiffnessLow)

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

    fun reset() {
        offsetPx = 0f
    }

    fun takesDrag(delta: Float, foldedPx: Float): Boolean = offsetPx > 0f || (delta > 0f && foldedPx == 0f)

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

@Composable
internal fun rememberCardCloseDragState(): CardCloseDragState {
    val windowHeightPx = LocalWindowInfo.current.containerSize.height.toFloat()
    val state = remember(windowHeightPx) { CardCloseDragState(windowHeightPx) }

    // AnimatedContent gives a card picked again mid-fade its old composition, swipe offset included.
    val leaving = isPocketScreenLeaving()
    LaunchedEffect(leaving) {
        if (!leaving) state.reset()
    }
    return state
}
