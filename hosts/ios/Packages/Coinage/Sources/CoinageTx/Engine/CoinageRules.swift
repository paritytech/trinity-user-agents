import DurableTransactions
import Foundation

/// The facts about coins and vouchers the oracle answers with, drawn from the graph and the evidence.
///
/// Every one is positive-form and paired with its opposite. A read is three-valued, so `!exists` would
/// mean "absent or unreadable" and a network error would start deciding things. The ladder that used to
/// consume these is the engine's now; what is left is the part that is actually about coins and vouchers.
public enum CoinageRules {
    /// The asset is still there to be spent. A voucher must also read not-unloaded: presence alone cannot
    /// tell a live one from one already consumed at that head.
    public static func available(_ input: CoinageTxInput, _ evidence: ChainEvidence, atFinalized: Bool) -> Bool {
        if input.isCoin {
            return evidence.exists(input.publicKey, atFinalized: atFinalized)
        }
        return evidence.exists(input.publicKey, atFinalized: atFinalized)
            && evidence.isNotUnloaded(input.publicKey, atFinalized: atFinalized)
    }

    /// Nothing could have removed this output, so its absence is meaningful.
    public static func noPotentialConsumers(
        _ output: OwnAsset,
        _ dag: CoinageEntryDag,
        _ evidence: ChainEvidence
    ) -> Bool {
        if dag.isHandedOff(output.publicKey) { return false }
        if spent(output, dag, evidence) { return false }
        return dag.consumers(output.publicKey).allSatisfy { $0.status == .failure }
    }

    /// Once established this is permanent: a terminal status never changes, and a coin absent at a
    /// finalized head can never come back, because addresses are never reused.
    public static func spent(_ output: OwnAsset, _ dag: CoinageEntryDag, _ evidence: ChainEvidence) -> Bool {
        let consumedByFinalized = dag.consumers(output.publicKey).contains { $0.status == .finalizedSuccess }
        let provenConsumed = !output.isCoin && evidence.isUnloaded(output.publicKey, atFinalized: true)
        return consumedByFinalized || provenConsumed || spentByAbsence(output, dag, evidence)
    }

    /// Absence read as consumption, guarded by `isCoin` (a voucher's disappearance may be ring cleaning)
    /// and the minter's window having closed (a coin minted above a shallow finalized head reads absent
    /// simply because it does not exist there yet).
    public static func spentByAbsence(_ output: OwnAsset, _ dag: CoinageEntryDag, _ evidence: ChainEvidence) -> Bool {
        guard output.isCoin, let minter = dag.minter(output.publicKey) else { return false }
        return minter.status == .finalizedSuccess
            && evidence.absent(output.publicKey, atFinalized: true)
            && evidence.windowClosed(minter)
    }

    /// Every input is a coin we minted ourselves, proven to have existed and old enough that its absence
    /// now is meaningful — what resolves the ambiguity in the completed-by-consumption case: such a coin
    /// also reads absent before it was ever minted.
    public static func hasOnlyProvenOwnCoinInputs(
        _ entry: CoinageTxEntry,
        _ dag: CoinageEntryDag,
        _ evidence: ChainEvidence
    ) -> Bool {
        guard !entry.inputs.isEmpty else { return false }

        return entry.inputs.allSatisfy { input in
            guard input.isCoin, input.isOwn, !dag.isHandedOff(input.publicKey),
                  let minter = dag.minter(input.publicKey), minter.status == .finalizedSuccess
            else { return false }
            return evidence.windowClosed(minter)
        }
    }

    /// A successor that consumed our output proves the output existed, and an output exists only if the
    /// entry minting it executed — positive evidence that arrives before the entry's own window closes.
    /// The opposite direction needs no rule: a failed entry's outputs never existed, so its successors are
    /// decided by their own mortality, in parallel.
    public static func successorProvesCompletion(_ entry: CoinageTxEntry, _ dag: CoinageEntryDag) -> Bool {
        dag.successors(entry).contains { $0.status == .finalizedSuccess }
    }
}
