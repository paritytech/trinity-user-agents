package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.animation.core.animate
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Velocity
import kotlin.math.roundToInt

/**
 * Tracks how much of the reveal-on-pull-down header is currently showing.
 *
 * [offsetPx] runs from 0 (fully hidden above the viewport) to [maxOffsetPx] (fully revealed).
 */
@Stable
internal class SearchRevealState(initialOffsetPx: Float = 0f) {
    var offsetPx by mutableFloatStateOf(initialOffsetPx)
        private set

    var maxOffsetPx by mutableFloatStateOf(0f)

    /** Applies [delta] within bounds and returns the portion actually used. */
    fun drag(delta: Float): Float {
        val target = (offsetPx + delta).coerceIn(0f, maxOffsetPx)
        val consumed = target - offsetPx
        offsetPx = target

        return consumed
    }

    /** Settles to fully open or fully hidden once the gesture ends. */
    suspend fun settle() {
        val target = if (offsetPx > maxOffsetPx / 2f) maxOffsetPx else 0f
        if (offsetPx == target) return

        animate(initialValue = offsetPx, targetValue = target) { value, _ -> offsetPx = value }
    }

    companion object {
        val Saver: Saver<SearchRevealState, Float> = Saver(
            save = { it.offsetPx },
            restore = { SearchRevealState(it) }
        )
    }
}

@Composable
internal fun rememberSearchRevealState(): SearchRevealState =
    rememberSaveable(saver = SearchRevealState.Saver) { SearchRevealState() }

/**
 * Hosts a [header] that stays hidden above [content] until the user drags downwards.
 *
 * The reveal is driven by nested scroll rather than by making the header the first list item:
 * a list shorter than the viewport has no scroll range of its own, but still forwards the whole
 * drag to [NestedScrollConnection.onPostScroll], so the gesture works with a short or empty list too.
 */
@Composable
internal fun SearchRevealContainer(
    state: SearchRevealState,
    header: @Composable () -> Unit,
    content: @Composable () -> Unit,
) {
    val connection = rememberSearchRevealConnection(state)

    Box(
        modifier = Modifier
            .fillMaxSize()
            .clipToBounds()
            .nestedScroll(connection)
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .layout { measurable, constraints ->
                    val placeable = measurable.measure(constraints)
                    // Placement uses the measured height directly so the header never flashes
                    // at the top before its size is known.
                    state.maxOffsetPx = placeable.height.toFloat()

                    layout(placeable.width, placeable.height) {
                        placeable.place(0, (state.offsetPx - placeable.height).roundToInt())
                    }
                }
        ) {
            header()
        }

        Box(
            modifier = Modifier
                .fillMaxSize()
                .graphicsLayer { translationY = state.offsetPx }
        ) {
            content()
        }
    }
}

@Composable
private fun rememberSearchRevealConnection(state: SearchRevealState): NestedScrollConnection {
    return remember(state) {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                // Dragging up: collapse the header before the list itself starts scrolling.
                if (available.y >= 0f) return Offset.Zero

                return Offset(0f, state.drag(available.y))
            }

            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                // Only the downward drag the list could not use reaches here — either it sits at the
                // top or it cannot scroll at all, which is exactly when the header should appear.
                if (available.y <= 0f) return Offset.Zero

                return Offset(0f, state.drag(available.y))
            }

            override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity {
                state.settle()

                return Velocity.Zero
            }
        }
    }
}
