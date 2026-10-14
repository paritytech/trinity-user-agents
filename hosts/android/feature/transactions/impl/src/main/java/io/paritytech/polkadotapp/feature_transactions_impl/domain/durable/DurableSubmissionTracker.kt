package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic
import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.chains.multiNetwork.getChain
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.chains.util.extrinsicHash
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.PreSubmissionValidationFailed
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ResubmitWhenValidFactory
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableFailureKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainViewFactory
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxCompletionOracle
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.Verdict
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

/**
 * Follows one extrinsic from submission. Its purpose is latency: it learns of inclusion and finality faster
 * than polling would.
 *
 * It proposes; it does not write. Every status change goes through the same compare-and-set the recovery
 * pass uses, so the guards apply uniformly.
 *
 * It owns the transactions it watches, and ownership is one-shot: released exactly once, never taken back —
 * not on a resubmission, not on anything. Release drops it, stops the subscription and triggers a pass.
 */
class DurableSubmissionTracker @Inject constructor(
    private val oracles: Map<String, @JvmSuppressWildcards TxCompletionOracle>,
    private val chainRegistry: ChainRegistry,
    private val extrinsicService: ExtrinsicService,
    private val repository: DurableTxRepository,
    private val verdictWriter: DurableVerdictWriter,
    private val chainViewFactory: PinnedChainViewFactory,
    private val submissionOwned: SubmissionOwnedTransactions,
    private val resubmitWhenValidFactory: ResubmitWhenValidFactory,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
) {
    fun watch(
        scope: CoroutineScope,
        id: DurableTxId,
        extrinsic: SendableExtrinsic,
        onReleased: () -> Unit,
    ) {
        val txHash = extrinsic.extrinsicHex.extrinsicHash()

        scope.launch {
            runCatching { follow(id, txHash, extrinsic) }
                .onFailure { durabilityLogW("${logId(id)} submission-watch-failed error=$it") }

            submissionOwned.release(id, txHash)

            // Recovery is for transactions nobody has decided. A watch that ended by writing a terminal
            // verdict has already done the deciding, and a terminal row is never rewritten — so asking for
            // a pass would schedule a worker and pin a chain view to re-derive an answer that exists.
            if (needsRecovery(id)) onReleased()
        }
    }

    /** Unreadable counts as needing recovery: the pass re-reads it, where guessing here could strand it. */
    private suspend fun needsRecovery(id: DurableTxId): Boolean {
        val status = repository.getStatus(id).getOrNull()

        // One waiting to be built is the executor's, and no pass could decide it.
        if (status?.awaitsVerdict == false) {
            durabilityLogD("${logId(id)} recovery-skipped reason=decided status=$status")

            return false
        }

        return true
    }

    private suspend fun follow(id: DurableTxId, txHash: TransactionHash, extrinsic: SendableExtrinsic) {
        val chain = chainRegistry.getChain(chainOf(id))

        // Held for as long as the watch lives. This subscription is the only thing following the
        // transaction while it is in flight, and a connection torn down because the app went to background
        // would end it — handing it to the recovery pass to decide the slow way, over its whole window.
        chainConnectionRefCounter.withConnectionEnabled(chain.id, CONNECTION_LABEL) {
            watchSubmission(id, txHash, chain, extrinsic)
        }
    }

    private suspend fun watchSubmission(
        id: DurableTxId,
        txHash: TransactionHash,
        chain: Chain,
        extrinsic: SendableExtrinsic,
    ) = coroutineScope {
        // Only a post-pool invalidation is worth resubmitting: anything else releases the transaction to
        // the recovery pass, which is the one place allowed to decide it.
        val recoveryStrategy = resubmitWhenValidFactory.createForTxInvalidation(chain.id, RECOVERY_MAX_ATTEMPTS)
        val events = Channel<ExtrinsicStatus>(Channel.BUFFERED)

        val pump = launch {
            runCatching {
                extrinsicService.submitAndWatchBuiltExtrinsic(chain, extrinsic, recoveryStrategy)
                    .collect(events::send)
            }
            events.close()
        }

        try {
            while (true) {
                // A dead subscription is handled as the watcher releasing rather than as someone else
                // invalidating it. The timeout always fires long before the extrinsic can no longer execute.
                val status = withTimeoutOrNull(SILENCE_TIMEOUT) {
                    events.receiveCatching().getOrNull()
                }

                if (status == null) {
                    durabilityLogW("${logId(id)} submission-abandoned reason=silence-timeout")
                    break
                }

                // Only the status that ends the watch goes to info: a batch of twenty vouchers reports four
                // times each, which would spend the whole breadcrumb budget on one operation.
                val line = "${logId(id)} submission-status ${status.describe()}"
                if (status.terminal) durabilityLogI(line) else durabilityLogD(line)

                if (handle(id, txHash, status)) break
            }
        } finally {
            pump.cancel()
        }
    }

    /** Returns true when the transaction is done being watched. */
    private suspend fun handle(id: DurableTxId, txHash: TransactionHash, status: ExtrinsicStatus): Boolean = when (status) {
        // Pre-inclusion states carry no evidence either way. They must not lower a transaction that has
        // some: the resubmission path keeps consuming this flow after an inclusion, so one of these can
        // arrive behind an InBlock, and clearing the record there would withdraw its effects on nothing.
        is ExtrinsicStatus.Future,
        is ExtrinsicStatus.Ready,
        is ExtrinsicStatus.Broadcast -> false

        // Not finalized, so a terminal verdict must not rest on it: a proven failure here proposes nothing
        // and the transaction stays PENDING for the pass to decide.
        is ExtrinsicStatus.InBlock -> {
            val at = blockOf(id, status.blockHash)
            val outcome = at?.let { dispatchOutcome(status.blockHash, id) }

            durabilityLogD("${logId(id)} in-block block=${at?.blockNumber} outcome=$outcome")

            if (outcome == ExtrinsicOutcome.SUCCESS) {
                propose(id, txHash, Verdict(DurableTxStatus.PENDING_SUCCESS, successDetectedAt = at, failure = null))
            }
            false
        }

        is ExtrinsicStatus.Retracted -> {
            clearRecordIfItNames(id, txHash, status.blockHash)
            false
        }

        is ExtrinsicStatus.Finalized -> {
            val outcome = dispatchOutcome(status.blockHash, id)

            durabilityLogD("${logId(id)} finalized outcome=$outcome")

            when (outcome) {
                ExtrinsicOutcome.SUCCESS ->
                    propose(id, txHash, Verdict(DurableTxStatus.FINALIZED_SUCCESS, blockOf(id, status.blockHash), failure = null))

                ExtrinsicOutcome.FAILURE ->
                    propose(id, txHash, Verdict(DurableTxStatus.FAILURE, successDetectedAt = null, DurableFailureKind.DISPATCH_FAILED))

                null -> Unit
            }
            true
        }

        // Recovery has already had its chance to resubmit by the time these surface as terminal.
        is ExtrinsicStatus.Dropped,
        is ExtrinsicStatus.Invalid,
        is ExtrinsicStatus.Usurped -> true

        is ExtrinsicStatus.FailedToSubmit -> {
            // Validation runs before the extrinsic is handed to a node and a rejection there is only ever
            // propagated when recovery declined to resubmit, so nothing can ever include these bytes:
            // finalized-grade evidence without waiting for finality.
            if (status.exception is PreSubmissionValidationFailed) {
                propose(id, txHash, Verdict(DurableTxStatus.FAILURE, successDetectedAt = null, DurableFailureKind.REJECTED))
            }
            true
        }

        is ExtrinsicStatus.Other -> false
    }

    /**
     * The record is cleared only when it names the block that was retracted; the status is lowered with it,
     * because leaving PENDING_SUCCESS behind with no evidence would keep its effects trusted for a whole
     * mortality window on the strength of a block that no longer exists.
     */
    private suspend fun clearRecordIfItNames(id: DurableTxId, txHash: TransactionHash, blockHash: String) {
        val entry = repository.getEntry(id).getOrNull() ?: return
        if (entry.successDetectedAt?.blockHash != blockHash) return

        propose(id, txHash, Verdict(DurableTxStatus.PENDING, successDetectedAt = null, failure = null))
    }

    /**
     * A terminal row is never rewritten, so a late event cannot un-fail a failed transaction; the
     * compare-and-set then covers a status that moved since it was read. A row whose attempt is no longer the
     * one watched is not this watch's to write: its bytes were proven unable to land and built again.
     */
    private suspend fun propose(id: DurableTxId, txHash: TransactionHash, verdict: Verdict) {
        val observed = repository.getEntry(id).getOrNull() ?: run {
            durabilityLogW("${logId(id)} proposal-skipped to=${verdict.status} reason=entry-unreadable")

            return
        }

        if (!observed.status.awaitsVerdict || observed.txHash != txHash) {
            durabilityLogD(
                "${logId(id)} proposal-skipped to=${verdict.status} reason=not-awaiting-verdict " +
                    "observed=${observed.status} attempt=${observed.txHash.shortHash()}"
            )

            return
        }

        durabilityLogD(
            "${logId(id)} proposing from=${observed.status} to=${verdict.status} " +
                "record=${verdict.successDetectedAt?.blockNumber ?: "none"}"
        )

        verdictWriter.write(observed, verdict)
            .onFailure { durabilityLogW("${logId(id)} proposal-write-failed to=${verdict.status} error=$it") }
    }

    private suspend fun dispatchOutcome(blockHash: String, id: DurableTxId): ExtrinsicOutcome? {
        val entry = repository.getEntry(id).getOrNull() ?: return null
        val view = chainViewFactory.pin(chainOf(id)).getOrNull() ?: return null

        return view.dispatchOutcomeAt(blockHash, entry.txHash).getOrNull()
    }

    private suspend fun blockOf(id: DurableTxId, blockHash: String): CheckpointBlock? {
        val view = chainViewFactory.pin(chainOf(id)).getOrNull() ?: return null
        val number = view.blockNumberAt(blockHash).getOrNull() ?: return null

        return CheckpointBlock(number, blockHash)
    }

    /** The chain a transaction lives on is its domain's, which its oracle declares. */
    private suspend fun chainOf(id: DurableTxId): ChainId {
        val domain = repository.getEntry(id).getOrNull()?.domainId?.value

        return oracles[domain]?.chainId ?: error("No registered oracle for domain $domain")
    }

    /** Falls back to the bare id when the row cannot be read — a log line must never fail the watch. */
    private suspend fun logId(id: DurableTxId): String =
        repository.getEntry(id).getOrNull()?.logId() ?: "entry=${id.value}"

    private companion object {
        const val CONNECTION_LABEL = "DurableSubmission"

        val SILENCE_TIMEOUT = 30.seconds

        /** Mirrors the platform default; the constant itself lives in a module this cannot depend on. */
        const val RECOVERY_MAX_ATTEMPTS = 3
    }
}

private fun ExtrinsicStatus.describe(): String = when (this) {
    is ExtrinsicStatus.InBlock -> "InBlock at=${blockHash.shortHash()}"
    is ExtrinsicStatus.Retracted -> "Retracted at=${blockHash.shortHash()}"
    is ExtrinsicStatus.Finalized -> "Finalized at=${blockHash.shortHash()}"
    is ExtrinsicStatus.FailedToSubmit -> "FailedToSubmit error=${exception::class.simpleName}"
    is ExtrinsicStatus.Usurped -> "Usurped by=${by.shortHash()}"
    is ExtrinsicStatus.Other -> "Other raw=$rawStatus"
    else -> this::class.simpleName.orEmpty()
}
