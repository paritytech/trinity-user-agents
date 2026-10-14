package io.paritytech.polkadotapp.feature_transactions.api.domain.durable

/**
 * A ready-made oracle for the commonest shape a domain has: one observation per transaction that, once
 * true, stays true — a value appended to a list, a slot claimed for a period, a flag set.
 *
 * A domain implements one batched read and gets the whole ladder: pre-finality success, terminal failure
 * once the window closes, and the block search as the backstop underneath both.
 *
 * ## When this is sound
 *
 * Only when both of these hold of the observation:
 *
 * - **Monotone** — once it reads true it never reads false again. If something can undo the effect, a later
 *   false reading would be reported as proof the transaction never ran.
 * - **Singly written** — this transaction is the only thing that can make it true. If anything else can,
 *   a true reading is not evidence about *this* transaction.
 *
 * An append to a set the app alone writes qualifies. A mutable cell, or a counter that rises on success and
 * falls again as something is consumed, does not — for those, answer only [effectsAt] where you are certain
 * and leave the rest absent, or implement [TxCompletionOracle] directly and never claim non-completion.
 *
 * A domain that cannot satisfy these can still use [TxCompletionOracle.Unobservable] and be decided by
 * history alone, which is correct and merely slower.
 */
abstract class MonotoneEffectOracle : TxCompletionOracle {
    /**
     * Whether each transaction's effect is observable at [at].
     *
     * One read for the whole pass, not one per transaction. A transaction **missing from the result** is a
     * read that did not answer: it decides nothing and is retried, which is what a transport error must do.
     * Returning `false` is a positive claim that the effect is not there — see the soundness note above.
     */
    protected abstract suspend fun effectsAt(
        transactions: List<DurableTxEntry>,
        at: CheckpointBlock,
    ): Map<DurableTxId, Boolean>

    final override suspend fun openPass(
        transactions: List<DurableTxEntry>,
        ledger: LedgerView,
        view: PinnedChainView,
    ): Result<TxCompletionOracle.PassScope> = runCatching {
        MonotoneScope(
            atFinalized = effectsAt(transactions, view.finalizedHead),
            atBest = effectsAt(transactions, view.bestHead),
        )
    }
}

private class MonotoneScope(
    private val atFinalized: Map<DurableTxId, Boolean>,
    private val atBest: Map<DurableTxId, Boolean>,
) : TxCompletionOracle.PassScope {
    override fun provenCompleted(tx: DurableTxEntry, head: HeadKind): Boolean = read(head)[tx.id] == true

    override fun provenNotCompleted(tx: DurableTxEntry, head: HeadKind): Boolean = read(head)[tx.id] == false

    private fun read(head: HeadKind) = when (head) {
        HeadKind.FINALIZED -> atFinalized
        HeadKind.BEST -> atBest
    }
}
