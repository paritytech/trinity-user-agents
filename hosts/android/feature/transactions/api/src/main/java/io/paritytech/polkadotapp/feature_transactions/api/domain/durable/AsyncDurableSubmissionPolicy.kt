package io.paritytech.polkadotapp.feature_transactions.api.domain.durable

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic

/**
 * Builds a durable transaction's extrinsic outside the call that registered it: for the first time when it was
 * scheduled, and again when an attempt is proven unable to land.
 *
 * A rebuild must consume and mint exactly what the domain registered for the transaction. The domain's rows
 * stay attached to the same id across attempts, and every lock and completion rule reads them.
 */
interface AsyncDurableSubmissionPolicy {
    /** The chain the built extrinsics are submitted to; kept connected while this policy works. */
    val chainId: ChainId

    /**
     * Whether [entry], whose attempt was proven unable to land because of [failure], should be built again
     * instead of failing.
     *
     * Asked while a verdict is being written, so it must not read the chain. Whether a rebuild is still
     * possible belongs to [prepareSubmission], which may take as long as it needs to find out. A failure that
     * would repeat on the same effects must not be retried indefinitely: nothing else bounds the loop.
     */
    suspend fun canRetry(entry: DurableTxEntry, params: DataByteArray, failure: DurableFailureKind): Boolean

    /**
     * Builds whichever of [transactions] it can. Every one of them shares a policy and a group, so work
     * common to them — pinned blocks, proofs, anything handed out per extrinsic — is done once per call.
     *
     * May suspend for as long as it waits for something on chain. A transaction absent from the result stays
     * waiting and comes back in a later call; [SubmissionPreparation.GiveUp] fails it for good. A failed
     * result is never a verdict: the call is made again after a backoff.
     */
    suspend fun prepareSubmission(
        transactions: List<ScheduledDurableTx>,
    ): Result<Map<DurableTxId, SubmissionPreparation>>
}

sealed interface SubmissionPreparation {
    class Ready(val extrinsic: EnrichedSendableExtrinsic) : SubmissionPreparation

    /** Nothing further can make this transaction land. */
    data object GiveUp : SubmissionPreparation
}
