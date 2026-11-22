package io.paritytech.polkadotapp.feature_products_api.model

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.layout.LayoutCoordinates
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.updateAndGet

/**
 * The finger's strokes on one keyed location pan since the current tree, for a cover whose brush follows that pan
 * (`CoverGrid`'s `brush`); excluded from serialized widget trees. Each stroke is its points as x, y pairs in px, in
 * the pan's own frame from its top-left corner whatever the layout direction. A tree drops them, as it drops the cells
 * cleared since. Each value is a fresh copy, never changed after it is published.
 */
class JsPanTrail {
    data class Value(val strokes: List<FloatArray>, val version: Long)

    private val mutableValue = MutableStateFlow(Value(emptyList(), 0))
    val value = mutableValue.asStateFlow()

    private val drawnState = mutableStateOf(mutableValue.value)

    /**
     * The same value as a snapshot state, for the draw phase, as [JsCoverCells.drawn]: the touch writes it, so the
     * frame that draws a move's cleared cells draws the stroke that cleared them.
     */
    val drawn: Value get() = drawnState.value

    /** Where the pan is laid out, so a cover inside it can place the strokes in its own frame. */
    @Volatile
    var frame: LayoutCoordinates? = null

    /** A new stroke, from where the finger touched down. */
    fun begin(x: Float, y: Float) {
        drawnState.value = mutableValue.updateAndGet { Value(it.strokes + floatArrayOf(x, y), it.version + 1) }
    }

    /** The finger reached [x], [y] on the current stroke; with no stroke begun, it starts one. */
    fun extend(x: Float, y: Float) {
        drawnState.value = mutableValue.updateAndGet { current ->
            val last = current.strokes.lastOrNull()
                ?: return@updateAndGet Value(listOf(floatArrayOf(x, y)), current.version + 1)
            if (last[last.size - 2] == x && last[last.size - 1] == y) return@updateAndGet current
            Value(current.strokes.dropLast(1) + (last + floatArrayOf(x, y)), current.version + 1)
        }
    }

    /** A new tree arrived: the strokes go with the cells it replaces. */
    fun reset() {
        drawnState.value = mutableValue.updateAndGet {
            if (it.strokes.isEmpty()) it else Value(emptyList(), it.version + 1)
        }
    }
}
