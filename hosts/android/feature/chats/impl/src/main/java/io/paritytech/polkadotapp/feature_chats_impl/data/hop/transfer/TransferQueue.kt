package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.WithRetryState
import kotlinx.coroutines.flow.Flow
import kotlin.time.Instant

interface TransferItemLifecycle<T : WithRetryState> {
    suspend fun process(item: T)
    suspend fun markDone(item: T)
    suspend fun markFailed(item: T, error: Throwable)
    suspend fun scheduleRetry(item: T, reschedule: RetryDecision.Reschedule)
}

interface TransferQueue<T : WithRetryState> : TransferItemLifecycle<T> {
    suspend fun nextPending(): T?

    suspend fun markInProgress(item: T)

    suspend fun soonestNextAttemptAt(): Instant?
}

interface ConcurrentTransferQueue<T : WithRetryState> : TransferItemLifecycle<T> {
    val maxInFlight: Int

    suspend fun claimNextDue(): T?

    suspend fun releaseInProgress()

    fun queueChanges(): Flow<Unit>

    suspend fun nextBackoffEndAt(): Instant?
}
