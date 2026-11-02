package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.WithRetryState
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Instant

class TransferQueueProcessor @Inject constructor(
    private val retryPolicy: HopTransferRetryPolicy,
    private val timeProvider: TimeProvider
) {
    suspend fun <T : WithRetryState> processPending(queue: TransferQueue<T>) {
        while (true) {
            val item = queue.nextPending() ?: break

            queue.markInProgress(item)

            process(queue, item)
        }
    }

    suspend fun <T : WithRetryState> processPendingConcurrently(queue: ConcurrentTransferQueue<T>) = coroutineScope {
        // Callers run one pass per queue at a time (unique work), so a row still in progress was left by a pass that died or was stopped.
        queue.releaseInProgress()

        val slots = Semaphore(queue.maxInFlight)
        val wake = Channel<Unit>(Channel.CONFLATED)

        val changesWatcher = launch {
            queue.queueChanges().collect { wake.trySend(Unit) }
        }

        while (true) {
            slots.acquire()
            val item = queue.claimNextDue()

            if (item == null) {
                slots.release()
                if (slots.availablePermits == queue.maxInFlight) break

                withTimeoutOrNull(waitUntil(queue.nextBackoffEndAt())) { wake.receive() }
                continue
            }

            launch {
                try {
                    process(queue, item)
                } finally {
                    slots.release()
                    wake.trySend(Unit)
                }
            }
        }

        changesWatcher.cancel()
    }

    private suspend fun <T : WithRetryState> process(queue: TransferItemLifecycle<T>, item: T) {
        runCancellableCatching { queue.process(item) }
            .onSuccess { queue.markDone(item) }
            .onFailure { error -> handleFailure(queue, item, error) }
    }

    private fun waitUntil(instant: Instant?): Duration {
        if (instant == null) return Duration.INFINITE

        return (instant - timeProvider.now()).coerceAtLeast(Duration.ZERO)
    }

    private suspend fun <T : WithRetryState> handleFailure(queue: TransferItemLifecycle<T>, item: T, error: Throwable) {
        when (val decision = retryPolicy.decide(item.retryState, error, timeProvider.now())) {
            is RetryDecision.Reschedule -> queue.scheduleRetry(item, decision)
            is RetryDecision.Terminal -> queue.markFailed(item, error)
            is RetryDecision.Ignore -> Unit
        }
    }
}
