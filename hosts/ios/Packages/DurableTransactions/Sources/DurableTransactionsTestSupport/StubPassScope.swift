import DurableTransactions
import Foundation
import SubstrateSdk

/// A ``TxCompletionPassScope`` answering from fixed id sets, so ladder tests state exactly what a domain
/// established without modelling why.
public struct StubPassScope: TxCompletionPassScope {
    public var completedAtFinalized: Set<DurableTxId>
    public var completedAtBest: Set<DurableTxId>
    public var notCompletedAtFinalized: Set<DurableTxId>
    public var notCompletedAtBest: Set<DurableTxId>

    public init(
        completedAtFinalized: Set<DurableTxId> = [],
        completedAtBest: Set<DurableTxId> = [],
        notCompletedAtFinalized: Set<DurableTxId> = [],
        notCompletedAtBest: Set<DurableTxId> = []
    ) {
        self.completedAtFinalized = completedAtFinalized
        self.completedAtBest = completedAtBest
        self.notCompletedAtFinalized = notCompletedAtFinalized
        self.notCompletedAtBest = notCompletedAtBest
    }

    /// A scope that established nothing: every transaction reaches the body search.
    public static let unknown = StubPassScope()

    public func provenCompleted(_ transaction: DurableTxEntry, at head: HeadKind) -> Bool {
        switch head {
        case .finalized: completedAtFinalized.contains(transaction.id)
        case .best: completedAtBest.contains(transaction.id)
        }
    }

    public func provenNotCompleted(_ transaction: DurableTxEntry, at head: HeadKind) -> Bool {
        switch head {
        case .finalized: notCompletedAtFinalized.contains(transaction.id)
        case .best: notCompletedAtBest.contains(transaction.id)
        }
    }
}

/// A ``TxCompletionOracle`` whose scope a test decides per pass, with the ledger view it was handed
/// available to the decision — enough to model a domain that reasons over other transactions' statuses.
public final class StubCompletionOracle: TxCompletionOracle, @unchecked Sendable {
    public let chainId: ChainId
    public var scopeProvider: @Sendable ([DurableTxEntry], any LedgerView) throws -> any TxCompletionPassScope
    public private(set) var openedPasses = 0

    public init(
        chainId: ChainId = "stub-chain",
        scope: @escaping @Sendable ([DurableTxEntry], any LedgerView) throws -> any TxCompletionPassScope = { _, _ in
            StubPassScope.unknown
        }
    ) {
        self.chainId = chainId
        scopeProvider = scope
    }

    public func openPass(
        transactions: [DurableTxEntry],
        ledger: any LedgerView,
        view _: any PinnedChainViewProtocol
    ) async throws -> any TxCompletionPassScope {
        openedPasses += 1
        return try scopeProvider(transactions, ledger)
    }
}
