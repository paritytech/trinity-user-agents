import Foundation
import SubstrateSdk

/// A ready-made oracle for the commonest shape a domain has: one observation per transaction that, once
/// true, stays true — a value appended to a list, a slot claimed for a period, a flag set.
///
/// A domain implements one batched read and gets the whole ladder: pre-finality success, terminal
/// failure once the window closes, and the block search as the backstop underneath both.
///
/// ## When this is sound
///
/// Only when both of these hold of the observation:
///
/// - **Monotone** — once it reads true it never reads false again. If something can undo the effect, a
///   later false reading would be reported as proof the transaction never ran.
/// - **Singly written** — this transaction is the only thing that can make it true. If anything else
///   can, a true reading is not evidence about *this* transaction.
///
/// An append to a set the app alone writes qualifies. A mutable cell, or a counter that rises on success
/// and falls again as something is consumed, does not — for those, answer `effectsAt` only where you are
/// certain and leave the rest absent, or implement ``TxCompletionOracle`` directly and never claim
/// non-completion. A domain that cannot satisfy these can still use ``UnobservableOracle`` and be
/// decided by history alone, which is correct and merely slower.
public struct MonotoneEffectOracle: TxCompletionOracle {
    /// Whether each transaction's effect is observable at a block.
    ///
    /// One read for the whole pass, not one per transaction. A transaction **missing from the result**
    /// is a read that did not answer: it decides nothing and is retried, which is what a transport error
    /// must do. Returning `false` is a positive claim that the effect is not there — see the soundness
    /// note above. A throw aborts the pass for the domain.
    public typealias EffectsRead = @Sendable (_ transactions: [DurableTxEntry], _ at: BlockRef) async throws
        -> [DurableTxId: Bool]

    public let chainId: ChainId
    private let effectsAt: EffectsRead

    public init(chainId: ChainId, effectsAt: @escaping EffectsRead) {
        self.chainId = chainId
        self.effectsAt = effectsAt
    }

    public func openPass(
        transactions: [DurableTxEntry],
        ledger _: any LedgerView,
        view: any PinnedChainViewProtocol
    ) async throws -> any TxCompletionPassScope {
        async let atFinalized = effectsAt(transactions, view.finalizedHead)
        async let atBest = effectsAt(transactions, view.bestHead)
        return try await MonotoneScope(atFinalized: atFinalized, atBest: atBest)
    }
}

private struct MonotoneScope: TxCompletionPassScope {
    let atFinalized: [DurableTxId: Bool]
    let atBest: [DurableTxId: Bool]

    func provenCompleted(_ transaction: DurableTxEntry, at head: HeadKind) -> Bool {
        read(head)[transaction.id] == true
    }

    func provenNotCompleted(_ transaction: DurableTxEntry, at head: HeadKind) -> Bool {
        read(head)[transaction.id] == false
    }

    private func read(_ head: HeadKind) -> [DurableTxId: Bool] {
        switch head {
        case .finalized: atFinalized
        case .best: atBest
        }
    }
}
