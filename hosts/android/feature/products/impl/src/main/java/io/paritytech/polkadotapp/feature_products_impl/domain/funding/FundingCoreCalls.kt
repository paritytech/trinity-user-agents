package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.common.di.modules.ProcessScope
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

// Serial so the core sees the user's steps and their read-backs in order, even across coroutines.
@Singleton
class FundingCoreCalls @Inject constructor(
    @ProcessScope scope: CoroutineScope,
    dispatchers: CoroutineDispatchers,
) {
    private val queue = Channel<suspend () -> Unit>(Channel.UNLIMITED)

    init {
        scope.launch(dispatchers.io) {
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

    fun launch(call: suspend () -> Unit) {
        queue.trySend(call)
    }

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
