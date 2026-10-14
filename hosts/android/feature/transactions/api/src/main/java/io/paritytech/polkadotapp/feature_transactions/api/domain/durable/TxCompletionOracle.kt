package io.paritytech.polkadotapp.feature_transactions.api.domain.durable

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId

/** Which head a question is asked at. */
enum class HeadKind { FINALIZED, BEST }

/** The statuses of one domain's transactions, as read at the start of a pass. */
interface LedgerView {
    val transactions: List<DurableTxEntry>

    fun statusOf(id: DurableTxId): DurableTxStatus?
}

/**
 * A domain's answer to the only two questions the ladder asks of it.
 *
 * Both are positive-form. Neither firing means the reads established nothing, which is always sound: the
 * transaction falls through to the block-body search, which is ground truth. The engine never negates an
 * answer, so a transport error cannot become a verdict.
 */
interface TxCompletionOracle {
    /**
     * The chain this domain's transactions live on.
     *
     * Declared per domain rather than assumed globally, so a second domain on another chain needs no change
     * to the engine — and so a pass can pin one view per distinct chain rather than one per domain.
     */
    val chainId: ChainId

    /**
     * Every chain read the domain needs, for every transaction in this pass, issued here.
     *
     * The returned scope deliberately does not suspend: a domain cannot read the chain while the ladder is
     * running, so an N+1 inside rule evaluation is not expressible. Batching is the only shape that compiles.
     *
     * A failure aborts the pass for this domain; nothing is written and the next pass repeats it.
     */
    suspend fun openPass(
        transactions: List<DurableTxEntry>,
        ledger: LedgerView,
        view: PinnedChainView,
    ): Result<PassScope>

    interface PassScope {
        /**
         * Positive proof [tx] took effect at [head].
         *
         * Safe to assert optimistically at the best head: a reorg demotes it through Rule 0. This is also
         * the only source of pre-finality success — the body search is bounded at the finalized head and
         * can never establish it.
         */
        fun provenCompleted(tx: DurableTxEntry, head: HeadKind): Boolean

        /**
         * Positive proof [tx] has not taken effect at [head] — and could not have taken effect and been
         * erased since.
         *
         * Assert this only when the observed state is monotone and this transaction is its only writer.
         * At the finalized head, past mortality, the engine turns it into a terminal FAILURE that releases
         * whatever the domain locked and is never revised.
         *
         * Leaving it false costs a block-body search over the mortality window. Getting it wrong costs a
         * double spend, which is why it defaults to false and must be opted into.
         */
        fun provenNotCompleted(tx: DurableTxEntry, head: HeadKind): Boolean = false
    }

    companion object {
        /**
         * For a domain whose effects it cannot read — every transaction is then decided by the recorded
         * inclusion rule and the body search alone, which is correct, just slower.
         *
         * The chain is still required: without it there is no view to pin and nothing could be decided at
         * all, so a domain says where it lives even when it cannot say what it sees.
         */
        fun unobservableOn(chainId: ChainId): TxCompletionOracle = UnobservableOracle(chainId)
    }
}

private class UnobservableOracle(override val chainId: ChainId) : TxCompletionOracle {
    override suspend fun openPass(
        transactions: List<DurableTxEntry>,
        ledger: LedgerView,
        view: PinnedChainView,
    ): Result<TxCompletionOracle.PassScope> = Result.success(
        object : TxCompletionOracle.PassScope {
            override fun provenCompleted(tx: DurableTxEntry, head: HeadKind) = false
        }
    )
}
