package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.WithRetryState
import kotlin.time.Instant

interface TransferQueue<T : WithRetryState> {
    suspend fun nextPending(): T?
    suspend fun markInProgress(item: T)
    suspend fun process(item: T)
    suspend fun markDone(item: T)
    suspend fun markFailed(item: T, error: Throwable)
    suspend fun scheduleRetry(item: T, reschedule: RetryDecision.Reschedule)
    suspend fun soonestNextAttemptAt(): Instant?
}
