package io.paritytech.polkadotapp.feature_statement_store_impl.domain.notificationAllocator

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Serializes every read-then-claim of notification seqs in this process: scheduling capacity checks in the
 * allocator and seq reservation in the submission policy, whether driven by first delivery or by renewal.
 */
@Singleton
class NotificationSlotAllocationLock @Inject constructor() {
    private val mutex = Mutex()

    suspend fun <T> withLock(action: suspend () -> T): T = mutex.withLock { action() }
}
