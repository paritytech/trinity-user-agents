package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Puts an attempt on the wire and follows it: the submission watch that registration starts, and the same
 * watch for an attempt a policy built later, so a rebuilt transaction lands as fast as a fresh one.
 *
 * Owns the scope every watch runs in, which is what makes [close] a faithful crash in tests.
 */
@Singleton
class DurableSubmissionLauncher @Inject constructor(
    private val repository: DurableTxRepository,
    private val submissionTracker: DurableSubmissionTracker,
    private val submissionOwned: SubmissionOwnedTransactions,
    private val recoveryLoop: DurableRecoveryLoop,
    private val recoveryScheduler: DurableRecoveryScheduler,
    dispatchers: CoroutineDispatchers,
) {
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.computation)

    /** [id] must already be owned by submission for [extrinsic]'s attempt. */
    fun watch(id: DurableTxId, extrinsic: EnrichedSendableExtrinsic) {
        submissionTracker.watch(scope, id, extrinsic) { onSubmissionReleased() }
    }

    /**
     * Makes [extrinsic] the attempt of a transaction waiting to be built, then watches it. Returns whether the
     * transaction was still waiting.
     */
    suspend fun startAttempt(id: DurableTxId, extrinsic: EnrichedSendableExtrinsic): Result<Boolean> {
        val attempt = runCatching { extrinsic.toAttempt() }.getOrElse { return Result.failure(it) }

        // Owned before the row turns pending, so a pass can never evaluate the new attempt underneath its watch.
        // Bytes identical to an attempt already released cannot be owned again, and so must not be started.
        if (!submissionOwned.acquire(id, attempt.txHash)) {
            return Result.failure(IllegalStateException("Attempt ${attempt.txHash} of entry ${id.value} was already watched"))
        }

        return repository.startAttempt(id, attempt)
            .onSuccess { started ->
                if (started) watch(id, extrinsic) else submissionOwned.release(id, attempt.txHash)
            }
            .onFailure { submissionOwned.release(id, attempt.txHash) }
    }

    private fun onSubmissionReleased() {
        // Save a block of latency if a loop is already running, and schedule the worker in case none is.
        recoveryLoop.manualTrigger()
        recoveryScheduler.ensureRunning()
    }

    /** Cancels every watch this instance owns. */
    fun close() {
        scope.cancel()
    }
}
