package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Which transactions submission tracking currently owns, so the recovery pass skips exactly those and each
 * transaction has one writer at a time.
 *
 * Registration acquires inside its own database transaction, so a committed row always has an owner and a
 * pass can never evaluate a transaction the tracker is about to write. The pass asks per transaction at the
 * moment it reaches it rather than taking a copy up front — a copy would not contain one registered while
 * the pass was already running, and the pass would then evaluate it underneath its watcher.
 *
 * Deliberately volatile. A crash takes the set with it, and an empty set after restart is the correct answer
 * rather than stale state to reconcile: a durable set would strand transactions behind a watcher that no
 * longer exists. This is sound only because the client runs in one OS process — if WorkManager is ever
 * configured with `android:process`, this stops being an exclusivity mechanism.
 *
 * Ownership is one-shot per attempt: released exactly once and never re-acquired for the same bytes, not even
 * across a resubmission. A rebuilt transaction is a new attempt with a new hash, and is owned afresh.
 */
@Singleton
class SubmissionOwnedTransactions @Inject constructor() {
    private val mutex = Mutex()
    private val owned = mutableMapOf<Long, TransactionHash>()
    private val everReleased = mutableSetOf<Pair<Long, TransactionHash>>()

    /** Returns whether ownership was taken; an attempt already released is never owned again. */
    suspend fun acquire(id: DurableTxId, txHash: TransactionHash): Boolean = mutex.withLock {
        if (id.value to txHash !in everReleased) {
            owned[id.value] = txHash
            durabilityLogD("entry=${id.value} submission-ownership acquired hash=$txHash")
            true
        } else {
            durabilityLogW("entry=${id.value} submission-ownership acquire-ignored reason=already-released hash=$txHash")
            false
        }
    }

    suspend fun release(id: DurableTxId, txHash: TransactionHash) = mutex.withLock {
        if (owned[id.value] == txHash) owned -= id.value
        everReleased += id.value to txHash

        durabilityLogI("entry=${id.value} submission-ownership released hash=$txHash")
    }

    suspend fun isOwnedBySubmission(id: DurableTxId): Boolean = mutex.withLock {
        id.value in owned
    }
}
