package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock

/**
 * A render subscription's live offset, excluded from serialized widget trees.
 *
 * Answers arrive on the render stream's thread, off the main thread, so a frame that just sent a move can wait for
 * the answer ([awaitAnswer]) and lay it out before it is drawn.
 */
class JsGesturePosition {
    data class Value(val x: Int, val y: Int, val answer: Long, val attached: Boolean)

    private val mutableValue = MutableStateFlow(Value(0, 0, 0, true))
    val value = mutableValue.asStateFlow()

    private val lock = ReentrantLock()
    private val changed = lock.newCondition()

    /** Even an unchanged offset acknowledges the outstanding move. */
    fun receive(x: Int, y: Int) {
        lock.withLock {
            mutableValue.update { it.copy(x = x, y = y, answer = it.answer + 1) }
            changed.signalAll()
        }
    }

    /** An answer that moves nothing, such as shapes added to a mask: the outstanding move is answered all the same. */
    fun acknowledge() {
        lock.withLock {
            mutableValue.update { it.copy(answer = it.answer + 1) }
            changed.signalAll()
        }
    }

    /** A removed target or closed subscription can no longer send pending moves. */
    fun detach() {
        lock.withLock {
            mutableValue.update { it.copy(attached = false) }
            changed.signalAll()
        }
    }

    /**
     * Blocks for at most [timeoutNanos] until an answer after the one counted [after] arrives or the position is
     * detached; returns the newest value either way.
     */
    fun awaitAnswer(after: Long, timeoutNanos: Long): Value = lock.withLock {
        var left = timeoutNanos
        var current = mutableValue.value
        while (current.answer == after && current.attached && left > 0L) {
            left = changed.awaitNanos(left)
            current = mutableValue.value
        }
        current
    }
}
