import DurableTransactions
import Foundation
import SubstrateSdk

/// Coinage's resource graph, expressed as a completion oracle.
///
/// Answers the two questions the engine asks from the same facts the old rules were built from. What is
/// gone is the ordering and the mortality gating, which are true of any transaction and now live in the
/// engine's ladder.
public final class CoinageResourceOracle: TxCompletionOracle {
    public let chainId: ChainId

    private let ledger: any CoinageAssetLedgerProtocol
    private let reader: any CoinageStateReading
    private let collector = CoinageEvidenceCollector()

    public init(chainId: ChainId, ledger: any CoinageAssetLedgerProtocol, reader: any CoinageStateReading) {
        self.chainId = chainId
        self.ledger = ledger
        self.reader = reader
    }

    public func openPass(
        transactions: [DurableTxEntry],
        ledger ledgerView: any LedgerView,
        view: any PinnedChainViewProtocol
    ) async throws -> any TxCompletionPassScope {
        // Two flat reads for the whole pass, whatever the number of transactions in it. Statuses come
        // from the pass's snapshot, assets from coinage's rows.
        let assetsById = try await Dictionary(
            ledger.getAllEntries().map { ($0.id, (inputs: $0.inputs, outputs: $0.outputs)) },
            uniquingKeysWith: { first, _ in first }
        )
        let handedOff = try await ledger.getHandoffKeys()

        let entries = ledgerView.transactions.compactMap { transaction in
            assetsById[transaction.id]
                .map { CoinageTxEntry(entry: transaction, inputs: $0.inputs, outputs: $0.outputs) }
        }
        let dag = CoinageEntryDag(entries: entries, handedOff: handedOff)

        // Unchanged granularity: one collect per transaction, four concurrent reads inside each.
        var evidence: [CoinageTxId: ChainEvidence] = [:]
        for transaction in transactions {
            guard let entry = dag.entry(transaction.id) else { continue }
            evidence[transaction.id] = await collector.collect(entry: entry, reader: reader, heads: view.heads)
        }

        return CoinagePassScope(dag: dag, evidence: evidence)
    }
}

/// The old Rules 1–6 and 3b/4b, which were two questions asked at two heads.
struct CoinagePassScope: TxCompletionPassScope {
    let dag: CoinageEntryDag
    let evidence: [CoinageTxId: ChainEvidence]

    /// Rules 1 and 2 (an effect is visible) and 5 and 6 (every input we minted ourselves is gone), plus
    /// propagation: a finalized successor proves this entry ran. The engine runs two rounds per pass, so a
    /// successor promoted in the first is visible here in the second.
    func provenCompleted(_ transaction: DurableTxEntry, at head: HeadKind) -> Bool {
        guard let entry = dag.entry(transaction.id), let evidence = evidence[transaction.id] else { return false }
        let atFinalized = head == .finalized

        if evidence.executed(entry, atFinalized: atFinalized) { return true }

        // Propagation, which used to be a phase of its own. The engine runs two rounds per pass, so a
        // successor promoted in the first is visible here in the second — the same fixpoint the separate
        // phase reached by reloading the graph.
        if atFinalized, CoinageRules.successorProvesCompletion(entry, dag) { return true }

        return CoinageRules.hasOnlyProvenOwnCoinInputs(entry, dag, evidence)
            && entry.inputs.allSatisfy { evidence.absent($0.publicKey, atFinalized: atFinalized) }
    }

    /// Rules 3 and 4: an output nothing could have removed is absent, or an input is still there to be
    /// spent. Sound by construction — `noPotentialConsumers` enumerates every party that could have erased
    /// the effect: a peer holding the key, another of our transactions, a live claimant.
    func provenNotCompleted(_ transaction: DurableTxEntry, at head: HeadKind) -> Bool {
        guard let entry = dag.entry(transaction.id), let evidence = evidence[transaction.id] else { return false }
        let atFinalized = head == .finalized

        return entry.outputs.contains {
            CoinageRules.noPotentialConsumers($0, dag, evidence) && evidence.absent(
                $0.publicKey,
                atFinalized: atFinalized
            )
        } || entry.inputs.contains { CoinageRules.available($0, evidence, atFinalized: atFinalized) }
    }
}
