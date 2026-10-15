package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.spring
import androidx.compose.foundation.gestures.DraggableState
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

private const val DISMISS_DISTANCE_FRACTION = 0.25f
private val DismissVelocity = 800.dp

@Stable
internal class ScanPanelDragState(
    private val scope: CoroutineScope,
    private val dismissVelocityPx: Float,
    private val onDismiss: () -> Unit,
) {
    var offsetPx by mutableFloatStateOf(0f)
        private set

    var contentHeightPx = 0f

    private var settleJob: Job? = null

    val draggableState = DraggableState { delta -> dragBy(delta) }

    val nestedScrollConnection = object : NestedScrollConnection {
        override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
            if (source != NestedScrollSource.UserInput || available.y >= 0f || offsetPx == 0f) return Offset.Zero

            return Offset(0f, dragBy(available.y))
        }

        override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
            if (source != NestedScrollSource.UserInput || available.y <= 0f) return Offset.Zero

            return Offset(0f, dragBy(available.y))
        }

        override suspend fun onPreFling(available: Velocity): Velocity {
            if (offsetPx == 0f) return Velocity.Zero

            settle(available.y)
            return available
        }
    }

    fun reset() {
        settleJob?.cancel()
        offsetPx = 0f
    }

    fun settle(velocity: Float) {
        val dismiss = velocity > dismissVelocityPx || offsetPx > contentHeightPx * DISMISS_DISTANCE_FRACTION
        val target = if (dismiss) contentHeightPx else 0f

        settleJob?.cancel()
        settleJob = scope.launch {
            animate(
                initialValue = offsetPx,
                targetValue = target,
                initialVelocity = velocity,
                animationSpec = spring(stiffness = Spring.StiffnessMediumLow),
            ) { value, _ -> offsetPx = value.coerceIn(0f, contentHeightPx) }

            if (dismiss) onDismiss()
        }
    }

    private fun dragBy(delta: Float): Float {
        settleJob?.cancel()
        val previous = offsetPx
        offsetPx = (previous + delta).coerceIn(0f, contentHeightPx)
        return offsetPx - previous
    }
}

@Composable
internal fun rememberScanPanelDragState(onDismiss: () -> Unit): ScanPanelDragState {
    val scope = rememberCoroutineScope()
    val dismissVelocityPx = with(LocalDensity.current) { DismissVelocity.toPx() }
    val currentOnDismiss by rememberUpdatedState(onDismiss)

    return remember(scope, dismissVelocityPx) {
        ScanPanelDragState(scope, dismissVelocityPx) { currentOnDismiss() }
    }
}

internal fun Modifier.dragToDismiss(state: ScanPanelDragState): Modifier = this
    .clipToBounds()
    .layout { measurable, constraints ->
        val placeable = measurable.measure(constraints)
        state.contentHeightPx = placeable.height.toFloat()
        val height = (placeable.height - state.offsetPx.roundToInt()).coerceAtLeast(0)

        layout(placeable.width, height) {
            placeable.place(0, 0)
        }
    }
    .nestedScroll(state.nestedScrollConnection)
    .draggable(
        state = state.draggableState,
        orientation = Orientation.Vertical,
        onDragStopped = { velocity -> state.settle(velocity) },
    )
