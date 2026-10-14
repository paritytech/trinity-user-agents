package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.WithRetryState
import javax.inject.Inject
import kotlin.time.Clock

class TransferQueueProcessor @Inject constructor(
    private val retryPolicy: HopTransferRetryPolicy
) {
    suspend fun <T : WithRetryState> processPending(queue: TransferQueue<T>) {
        while (true) {
            val item = queue.nextPending() ?: break

            queue.markInProgress(item)

            runCancellableCatching { queue.process(item) }
                .onSuccess { queue.markDone(item) }
                .onFailure { error -> handleFailure(queue, item, error) }
        }
    }

    private suspend fun <T : WithRetryState> handleFailure(queue: TransferQueue<T>, item: T, error: Throwable) {
        when (val decision = retryPolicy.decide(item.retryState, error, Clock.System.now())) {
            is RetryDecision.Reschedule -> queue.scheduleRetry(item, decision)
            is RetryDecision.Terminal -> queue.markFailed(item, error)
            is RetryDecision.Ignore -> Unit
        }
    }
}
