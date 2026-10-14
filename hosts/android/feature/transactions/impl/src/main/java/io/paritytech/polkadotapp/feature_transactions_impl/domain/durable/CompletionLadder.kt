package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableFailureKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.HeadKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TransactionSearchResult
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxCompletionOracle
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.Verdict

sealed interface RuleOutcome {
    data class Decided(val verdict: Verdict) : RuleOutcome

    /** A read this transaction depended on failed. It keeps its status and its locks, and is retried. */
    data object Undecided : RuleOutcome
}

/**
 * Decides one transaction against one pinned view. Evaluated in order; the first rule that matches wins.
 *
 * The eight rules this replaces were four predicates evaluated at two heads. Asking the oracle per head
 * collapses that: completion at F and at B are Rules 1 and 2, non-completion at F and at B are Rules 3
 * and 4, and everything domain-shaped that used to be spelled out here now lives behind [scope].
 *
 * [view] is consulted only by the last rule, which searches block bodies for the transaction itself.
 * [recordedStillCanonical] is resolved by the pass, batched across every transaction that carries a record,
 * so Rule 0 costs no read of its own here — null is a read that failed, which aborts this transaction.
 */
suspend fun evaluateLadder(
    tx: DurableTxEntry,
    scope: TxCompletionOracle.PassScope,
    view: PinnedChainView,
    recordedStillCanonical: Boolean?,
): RuleOutcome {
    recordedInclusion(tx, scope, view, recordedStillCanonical)?.let { return it }

    // Completion is visible at the finalized head.
    if (scope.provenCompleted(tx, HeadKind.FINALIZED)) {
        return decided(tx, "Rule 1 completed at F", DurableTxStatus.FINALIZED_SUCCESS, tx.successDetectedAt, view)
    }

    // Completion is visible at the best head. The rule above is evaluated first and wins on the same
    // evidence, so the overlap only ever costs a weaker verdict.
    if (scope.provenCompleted(tx, HeadKind.BEST)) {
        return decided(tx, "Rule 2 completed at B", DurableTxStatus.PENDING_SUCCESS, view.bestHead, view)
    }

    val windowClosed = view.finalizedHead.blockNumber > tx.mortalityEnd

    // Proven not to have run, and it can no longer run.
    if (windowClosed && scope.provenNotCompleted(tx, HeadKind.FINALIZED)) {
        return decided(tx, "Rule 3 not completed at F", DurableTxStatus.FAILURE, successDetectedAt = null, view = view, failure = DurableFailureKind.EXPIRED)
    }

    // Short circuits, so a transaction with no positive evidence does not run a body search on every new
    // head. It must not fire once mortality has expired: past it the transaction has to reach the search,
    // which is the only thing left that can decide it.
    if (!windowClosed && scope.provenNotCompleted(tx, HeadKind.BEST)) {
        return decided(tx, "Rule 4 not completed at B", DurableTxStatus.PENDING, successDetectedAt = null, view = view)
    }

    return searchForTransaction(tx, view, windowClosed)
}

/** Nothing above could decide it, so look for the transaction itself. */
private suspend fun searchForTransaction(
    tx: DurableTxEntry,
    view: PinnedChainView,
    windowClosed: Boolean,
): RuleOutcome {
    val search = view.searchForTransaction(
        fromBlockNumber = tx.checkpoint.blockNumber,
        toBlockNumber = minOf(tx.mortalityEnd, view.finalizedHead.blockNumber),
        txHash = tx.txHash,
    )

    return when (search) {
        is TransactionSearchResult.Found -> when (search.outcome) {
            ExtrinsicOutcome.SUCCESS ->
                decided(tx, "Rule 5 found, dispatch succeeded", DurableTxStatus.FINALIZED_SUCCESS, search.block, view)

            // Inclusion is not success — an extrinsic can be applied and its dispatch still fail.
            ExtrinsicOutcome.FAILURE ->
                decided(tx, "Rule 5 found, dispatch failed", DurableTxStatus.FAILURE, successDetectedAt = null, view = view, failure = DurableFailureKind.DISPATCH_FAILED)

            null ->
                decided(tx, "Rule 5 found, outcome unreadable", DurableTxStatus.PENDING, successDetectedAt = null, view = view)
        }

        is TransactionSearchResult.NotFound ->
            if (search.wholeRangeRead && windowClosed) {
                decided(tx, "Rule 5 whole window read, absent", DurableTxStatus.FAILURE, successDetectedAt = null, view = view, failure = DurableFailureKind.EXPIRED)
            } else {
                decided(tx, "Rule 5 window incomplete", DurableTxStatus.PENDING, successDetectedAt = null, view = view)
            }
    }
}

/**
 * We already saw this transaction included somewhere; check that block is still real.
 *
 * The record is only ever written where completion is already proven, so this never re-asks whether the
 * transaction took effect. It covers the transaction whose effect a peer claims before it finalizes: the
 * effect is then gone, and the search cannot reach its block yet, so without the record it would fall back
 * to PENDING and lose whatever its effect made selectable for a full mortality window.
 */
private fun recordedInclusion(
    tx: DurableTxEntry,
    scope: TxCompletionOracle.PassScope,
    view: PinnedChainView,
    recordedStillCanonical: Boolean?,
): RuleOutcome? {
    val recorded = tx.successDetectedAt ?: return null
    val stillCanonical = recordedStillCanonical ?: run {
        durabilityLogW("${tx.logId()} rule=undecided reason=record-canonicality-unread record=${recorded.blockNumber}")

        return RuleOutcome.Undecided
    }

    if (!stillCanonical) {
        return when {
            // Asked before the best head, or a chain that reorgs its head between passes would keep
            // re-recording this transaction above the finalized head and never let it finalize at all.
            scope.provenCompleted(tx, HeadKind.FINALIZED) ->
                decided(tx, "Rule 0 record gone, completed at F", DurableTxStatus.FINALIZED_SUCCESS, view.finalizedHead, view)

            scope.provenCompleted(tx, HeadKind.BEST) ->
                decided(tx, "Rule 0 record gone, still at B", DurableTxStatus.PENDING_SUCCESS, view.bestHead, view)

            // Writes PENDING rather than only clearing the record: clearing alone would leave the
            // transaction PENDING_SUCCESS with no evidence behind it, and whatever its effect made
            // selectable would stay so for a full mortality window on the strength of a block that no
            // longer exists.
            else -> decided(tx, "Rule 0 record gone, demoted", DurableTxStatus.PENDING, successDetectedAt = null, view = view)
        }
    }

    return if (recorded.blockNumber <= view.finalizedHead.blockNumber) {
        decided(tx, "Rule 0 record canonical at F", DurableTxStatus.FINALIZED_SUCCESS, recorded, view)
    } else {
        decided(tx, "Rule 0 record canonical above F", DurableTxStatus.PENDING_SUCCESS, recorded, view)
    }
}

/** Every terminal path names itself, so a log line says which rule spoke and not merely what it concluded. */
private fun decided(
    tx: DurableTxEntry,
    rule: String,
    status: DurableTxStatus,
    successDetectedAt: CheckpointBlock?,
    view: PinnedChainView,
    failure: DurableFailureKind? = null,
): RuleOutcome {
    durabilityLogD(
        "${tx.logId()} rule=\"$rule\" -> $status f=${view.finalizedHead.blockNumber} " +
            "b=${view.bestHead.blockNumber} record=${successDetectedAt?.blockNumber ?: "none"}"
    )

    return RuleOutcome.Decided(Verdict(status, successDetectedAt, failure))
}
