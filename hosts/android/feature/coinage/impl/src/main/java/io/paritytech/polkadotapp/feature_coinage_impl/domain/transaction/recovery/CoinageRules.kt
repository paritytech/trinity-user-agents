package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.recovery

import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus

// ---- predicates over a pinned view -------------------------------------------------------------------
//
// Every one is positive-form and paired with its opposite. A read is three-valued, so `!exists` would mean
// "absent or unreadable" and a network error would start deciding things.
//
// The ladder that used to consume these is the engine's now. What is left is the part that is actually
// about coins and vouchers, which is what the oracle answers with.

fun ChainEvidence.exists(asset: LedgerAsset, atFinalized: Boolean): Boolean =
    presence(atFinalized)[asset.publicKey] == ChainPresence.PRESENT

fun ChainEvidence.absent(asset: LedgerAsset, atFinalized: Boolean): Boolean =
    presence(atFinalized)[asset.publicKey] == ChainPresence.ABSENT

/** Always false for an asset with no consumption proof: its absence is the only signal it can offer. */
fun ChainEvidence.provenConsumedOnChain(asset: LedgerAsset, atFinalized: Boolean = true): Boolean =
    asset.hasConsumptionProof && alias(atFinalized)[asset.publicKey] == AliasRead.UNLOADED

fun ChainEvidence.provenNotConsumed(asset: LedgerAsset, atFinalized: Boolean): Boolean =
    asset.hasConsumptionProof && alias(atFinalized)[asset.publicKey] == AliasRead.NOT_UNLOADED

/**
 * The asset is still there to be spent.
 *
 * An asset carrying a consumption proof must show it as unconsumed as well as present: presence alone
 * cannot tell a live one from one already consumed at that head. An asset without such a proof has nothing
 * further to demand, so presence is the whole of it.
 */
fun ChainEvidence.available(asset: LedgerAsset, atFinalized: Boolean): Boolean = when {
    asset.hasConsumptionProof -> exists(asset, atFinalized) && provenNotConsumed(asset, atFinalized)
    else -> exists(asset, atFinalized)
}

/** An effect of this transaction is visible: something it mints exists, or something it spends is consumed. */
fun ChainEvidence.executed(entry: LedgerEntry, atFinalized: Boolean): Boolean =
    entry.outputs.any { exists(it, atFinalized) } ||
        entry.inputs.any { provenConsumedOnChain(it, atFinalized) }

private fun ChainEvidence.presence(atFinalized: Boolean) =
    if (atFinalized) presenceAtFinalized else presenceAtBest

private fun ChainEvidence.alias(atFinalized: Boolean) =
    if (atFinalized) aliasAtFinalized else aliasAtBest

// ---- asset facts drawn from the graph and the view ----------------------------------------------------

/**
 * Once established this is permanent: a terminal status never changes, and an asset absent at a finalized
 * head can never come back, because its identity is never reused.
 */
private fun LedgerAsset.spent(dag: CoinageEntryDag, evidence: ChainEvidence): Boolean {
    // A transaction that succeeded consumed all of its inputs.
    val consumedByFinalized = dag.consumers(publicKey)
        .any { it.status == DurableTxStatus.FINALIZED_SUCCESS }

    return consumedByFinalized || evidence.provenConsumedOnChain(this) || spentByAbsence(dag, evidence)
}

/**
 * Absence read as consumption, with two guards: [absenceProvesConsumption], because an asset whose identity
 * can be removed for other reasons proves nothing by vanishing; and the minter's window having closed,
 * because an asset minted above a shallow finalized head reads absent for the ordinary reason that it does
 * not exist there yet.
 */
private fun LedgerAsset.spentByAbsence(dag: CoinageEntryDag, evidence: ChainEvidence): Boolean {
    if (!absenceProvesConsumption) return false
    val minter = dag.minter(publicKey) ?: return false

    return minter.status == DurableTxStatus.FINALIZED_SUCCESS &&
        evidence.absent(this, atFinalized = true) &&
        evidence.windowClosed(minter)
}

/** Nothing could have removed this output, so its absence is meaningful. */
fun LedgerAsset.noPotentialConsumers(dag: CoinageEntryDag, evidence: ChainEvidence): Boolean {
    if (dag.isHandedOff(publicKey)) return false
    if (spent(dag, evidence)) return false

    return dag.consumers(publicKey).none { it.status != DurableTxStatus.FAILURE }
}

/**
 * Every input is an asset we minted ourselves, whose absence is meaningful, proven to have existed and old
 * enough that its absence now says something. This is what resolves the ambiguity in the completed-by-
 * consumption case: such an asset also reads absent before it was ever minted.
 */
fun LedgerEntry.hasOnlyProvenOwnInputs(dag: CoinageEntryDag, evidence: ChainEvidence): Boolean {
    if (inputs.isEmpty()) return false

    return inputs.all { input ->
        val minter = dag.minter(input.publicKey)

        input.absenceProvesConsumption &&
            input.asset != null &&
            !dag.isHandedOff(input.publicKey) &&
            minter != null &&
            minter.status == DurableTxStatus.FINALIZED_SUCCESS &&
            evidence.windowClosed(minter)
    }
}

/** This transaction can no longer execute, so anything it did has already happened below the finalized head. */
private fun ChainEvidence.windowClosed(entry: LedgerEntry): Boolean =
    finalized.blockNumber > entry.mortalityEnd
