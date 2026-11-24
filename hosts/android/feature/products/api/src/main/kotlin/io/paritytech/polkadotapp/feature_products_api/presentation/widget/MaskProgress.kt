package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.view.HapticFeedbackConstants
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.LocalView
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskHaptic
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShape
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShapes
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlin.math.roundToInt

@Composable
internal fun MaskProgress(mask: JsModifier.Mask, bounds: Rect, density: Float, picture: () -> JsMaskShapes.Picture) {
    val currentMask by rememberUpdatedState(mask)
    val currentBounds by rememberUpdatedState(bounds)
    val currentDensity by rememberUpdatedState(density)
    val currentPicture by rememberUpdatedState(picture)
    val dispatch by rememberUpdatedState(LocalJsEventHandler.current)
    val view = LocalView.current
    val appearedAt = remember { System.nanoTime() }
    LaunchedEffect(mask.trail, mask.clearedShare, mask.haptics) {
        fun input() = MaskShareWatch.Input(
            currentMask.trail?.down ?: false,
            currentMask.trail?.lifts ?: 0,
            currentMask.channel?.trees ?: 0,
            currentMask.channel?.additions ?: 0,
        )
        val watch = MaskProgressWatch(input())
        fun play(haptic: JsMaskHaptic) {
            if (view.isHapticFeedbackEnabled) view.performHapticFeedback(haptic.platformType())
        }
        snapshotFlow {
            ProgressPicture(
                input(), currentPicture().shapes, currentMask.shapes,
                (currentBounds.width / currentDensity * 100).roundToInt(),
                (currentBounds.height / currentDensity * 100).roundToInt(),
                currentMask.channel?.treeArrivedAtNanos ?: appearedAt,
                currentMask.trail?.strokes?.isNotEmpty() == true,
            )
        }.collectLatest { drawn ->
            suspend fun check() {
                withFrameNanos {
                    val (covered, total) = MaskCoverage.count(drawn.shapes, drawn.width, drawn.height)
                    val (treeCovered, _) = MaskCoverage.count(drawn.treeShapes, drawn.width, drawn.height)
                    val result = watch.update(
                        MaskProgressWatch.Frame(drawn.input, covered, total, treeCovered, drawn.treeAtNanos, System.nanoTime(), drawn.hasFollowedShapes),
                        currentMask.follow != null, currentMask.clearedShare, currentMask.haptics,
                    )
                    if (result.step) currentMask.haptics?.step?.let(::play)
                    if (result.action) {
                        currentMask.haptics?.reach?.let(::play)
                        currentMask.clearedShare?.let { dispatch(it.action, JsUiEvent.Type.ButtonClick) }
                    }
                }
            }
            check()
            while (true) {
                val deadline = watch.nextRescueAtNanos ?: break
                val remaining = (deadline - System.nanoTime()).coerceAtLeast(0)
                delay((remaining + 999_999) / 1_000_000)
                check()
            }
        }
    }
}

private data class ProgressPicture(
    val input: MaskShareWatch.Input,
    val shapes: List<JsMaskShape>,
    val treeShapes: List<JsMaskShape>,
    val width: Int,
    val height: Int,
    val treeAtNanos: Long,
    val hasFollowedShapes: Boolean,
)

internal fun JsMaskHaptic.platformType(): Int = when (this) {
    JsMaskHaptic.TICK -> HapticFeedbackConstants.CLOCK_TICK
    JsMaskHaptic.LIGHT -> HapticFeedbackConstants.VIRTUAL_KEY
    JsMaskHaptic.MEDIUM -> HapticFeedbackConstants.CONTEXT_CLICK
    JsMaskHaptic.HEAVY -> HapticFeedbackConstants.LONG_PRESS
    JsMaskHaptic.SUCCESS -> HapticFeedbackConstants.CONFIRM
}
