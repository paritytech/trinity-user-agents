package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.recovery

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageAssetLedger
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageStateReaderFactory
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerEntry
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.HeadKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.LedgerView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxCompletionOracle
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Coinage's resource graph, expressed as a completion oracle.
 *
 * Nothing about the evidence or the predicates changed when the ladder moved out of this module: this
 * answers the two questions the engine asks, and the answers are the same facts the old Rules 1–6 were
 * built from. What is gone is the ordering and the mortality gating, which are true of any transaction and
 * now live in one place.
 */
@Singleton
class CoinageResourceOracle @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val assetLedger: CoinageAssetLedger,
    private val stateReaderFactory: CoinageStateReaderFactory,
    private val evidenceCollector: CoinageEvidenceCollector,
) : TxCompletionOracle {
    override val chainId: ChainId get() = chainAssetProvider.chainId()

    override suspend fun openPass(
        transactions: List<DurableTxEntry>,
        ledger: LedgerView,
        view: PinnedChainView,
    ): Result<TxCompletionOracle.PassScope> = runCatching {
        // Two flat reads for the whole pass, whatever the number of transactions in it.
        val assets = assetLedger.assetsOf(ledger.transactions.map { it.id }).getOrThrow()
        val handedOff = assetLedger.getHandoffKeys().getOrThrow()

        val entries = ledger.transactions.mapNotNull { entry ->
            assets[entry.id]?.let { LedgerEntry(entry, it.inputs, it.outputs) }
        }
        val dag = CoinageEntryDag(entries, handedOff)

        val reader = stateReaderFactory.create(view)

        // Unchanged granularity: one collect() per transaction, five concurrent reads inside each.
        val evidence = transactions.associate { entry ->
            entry.id to evidenceCollector.collect(dag.entryOf(entry.id), reader, view)
        }

        CoinagePassScope(dag, evidence)
    }
}

internal class CoinagePassScope(
    private val dag: CoinageEntryDag,
    private val evidence: Map<CoinageTransactionId, ChainEvidence>,
) : TxCompletionOracle.PassScope {
    /**
     * The old Rules 1 and 2 (an effect is visible) and 5 and 6 (every input we minted ourselves is gone),
     * which were the same two questions asked at two heads.
     */
    override fun provenCompleted(tx: DurableTxEntry, head: HeadKind): Boolean {
        val entry = dag.entryOf(tx.id)
        val evidence = evidence[tx.id] ?: return false
        val atFinalized = head == HeadKind.FINALIZED

        if (evidence.executed(entry, atFinalized)) return true

        // Propagation, which used to be a phase of its own. The engine runs two rounds per pass, so a
        // successor promoted in the first is visible here in the second — the same fixpoint the separate
        // phase reached by reloading the graph.
        if (atFinalized && dag.successorProvesCompletion(entry)) return true

        return entry.hasOnlyProvenOwnInputs(dag, evidence) &&
            entry.inputs.all { evidence.absent(it, atFinalized) }
    }

    /**
     * The old Rules 3 and 4: an output nothing could have removed is absent, or an input is still there to
     * be spent.
     *
     * Sound by construction rather than by review — [noPotentialConsumers] enumerates every party that
     * could have erased the effect: a peer holding the key, another of our transactions, a live claimant.
     */
    override fun provenNotCompleted(tx: DurableTxEntry, head: HeadKind): Boolean {
        val entry = dag.entryOf(tx.id)
        val evidence = evidence[tx.id] ?: return false
        val atFinalized = head == HeadKind.FINALIZED

        return entry.outputs.any {
            it.noPotentialConsumers(dag, evidence) && evidence.absent(it, atFinalized)
        } || entry.inputs.any { evidence.available(it, atFinalized) }
    }
}

/**
 * A successor that consumed our output proves the output existed, and an output exists only if the
 * transaction minting it executed. That is positive evidence, and it arrives before the transaction's own
 * window closes.
 *
 * The opposite direction needs no rule: a failed transaction's outputs never existed, so its successors are
 * decided by their own mortality, in parallel, within one window rather than one window per hop.
 */
internal fun CoinageEntryDag.successorProvesCompletion(entry: LedgerEntry): Boolean =
    successors(entry).any { it.status == DurableTxStatus.FINALIZED_SUCCESS }
