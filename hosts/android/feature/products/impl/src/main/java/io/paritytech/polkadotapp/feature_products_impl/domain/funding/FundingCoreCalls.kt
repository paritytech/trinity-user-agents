package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import timber.log.Timber

/**
 * Runs calls into the core one after another, off the caller's thread, in the order they were made. The core must see
 * the user's steps, and the read-backs of what it said changed, in the order they happened even when they come from
 * separate coroutines.
 */
class FundingCoreCalls(scope: CoroutineScope) {
    private val queue = Channel<suspend () -> Unit>(Channel.UNLIMITED)

    init {
        scope.launch {
            for (call in queue) {
                try {
                    call()
                } catch (cancellation: CancellationException) {
                    throw cancellation
                } catch (failure: Exception) {
                    Timber.e(failure, "Funding core call failed")
                }
            }
        }
    }

    /** Queues [call] after every call made before it. */
    fun launch(call: suspend () -> Unit) {
        queue.trySend(call)
    }

    /** Queues [call] after every call made before it, and answers what it returned. */
    suspend fun <T> run(call: suspend () -> T): T {
        val answer = CompletableDeferred<T>()
        queue.trySend {
            try {
                answer.complete(call())
            } catch (failure: Exception) {
                answer.completeExceptionally(failure)
            }
        }
        return answer.await()
    }
}
