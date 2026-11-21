package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import java.util.BitSet

/**
 * The cells a worker cleared on one keyed cover since its last tree (`RendererNode.ClearCells`), excluded from
 * serialized widget trees. Each value is a fresh copy, never changed after it is published.
 */
class JsCoverCells {
    data class Value(val cells: BitSet, val version: Long)

    private val mutableValue = MutableStateFlow(Value(BitSet(), 0))
    val value = mutableValue.asStateFlow()

    /** Adds [cells]; one outside any grid is ignored. */
    fun clear(cells: List<Int>) {
        mutableValue.update { current ->
            val next = current.cells.clone() as BitSet
            cells.filter { it in 0 until MAX_CELLS }.forEach(next::set)
            Value(next, current.version + 1)
        }
    }

    /** A new tree arrived: it clears the cover itself. */
    fun reset() {
        mutableValue.update { Value(BitSet(), it.version + 1) }
    }

    companion object {
        /** Most cells a side may have, as the renderer allows. */
        const val MAX_SIDE = 128
        const val MAX_CELLS = MAX_SIDE * MAX_SIDE
    }
}
