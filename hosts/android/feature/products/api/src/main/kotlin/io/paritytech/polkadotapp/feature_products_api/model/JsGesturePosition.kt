package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/** A render subscription's live offset, excluded from serialized widget trees. */
class JsGesturePosition {
    data class Value(val x: Int, val y: Int, val answer: Long, val attached: Boolean)

    private val mutableValue = MutableStateFlow(Value(0, 0, 0, true))
    val value = mutableValue.asStateFlow()

    /** Even an unchanged offset acknowledges the outstanding move. */
    fun receive(x: Int, y: Int) {
        mutableValue.update { it.copy(x = x, y = y, answer = it.answer + 1) }
    }

    /** A removed target or closed subscription can no longer send pending moves. */
    fun detach() {
        mutableValue.update { it.copy(attached = false) }
    }
}
