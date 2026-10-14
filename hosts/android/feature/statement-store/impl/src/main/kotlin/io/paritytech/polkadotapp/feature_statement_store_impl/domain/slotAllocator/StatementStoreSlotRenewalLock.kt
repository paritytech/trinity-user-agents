package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Process-wide mutual exclusion between the in-process allocator path and the periodic
 * [StatementStoreSlotRenewalWorker]. Both inject this singleton and call [withLock] —
 * the first to grab the mutex runs; the other suspends. Cooperative coordination via
 * shared state, no external observer needed.
 */
@Singleton
class StatementStoreSlotRenewalLock @Inject constructor() {
    private val mutex = Mutex()

    suspend fun <T> withLock(action: suspend () -> T): T = mutex.withLock { action() }
}
