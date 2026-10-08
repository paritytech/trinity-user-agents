package io.paritytech.polkadotapp.common.utils

import io.paritytech.polkadotapp.common.domain.errors.UserCancellation
import kotlinx.coroutines.CompletableDeferred
import kotlin.coroutines.cancellation.CancellationException

/**
 * The answer a prompt owes the caller waiting in [await]. A caller that stops waiting withdraws it, which tells the
 * prompt through [awaitWithdrawal] that nobody is left to answer.
 */
class WithdrawableAnswer<T> {
    private val answer = CompletableDeferred<T>()
    private val withdrawal = CompletableDeferred<Unit>()

    val isWithdrawn: Boolean
        get() = withdrawal.isCompleted

    fun deliver(value: T) {
        answer.complete(value)
    }

    fun withdraw() {
        withdrawal.complete(Unit)
    }

    /** Runs [open] to show the prompt, then waits for its answer. A caller cancelled anywhere in between withdraws it. */
    suspend fun await(open: suspend () -> Unit): T = try {
        open()
        answer.await()
    } catch (e: CancellationException) {
        withdraw()
        throw e
    }

    suspend fun awaitWithdrawal() {
        withdrawal.await()
    }
}

/** An answer refused because its caller already withdrew the request; screens stay silent about it. */
class WithdrawnByCaller : IllegalStateException("The caller withdrew this request"), UserCancellation
