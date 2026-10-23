import Foundation

/// The outcome of a strategy's foreground preparation.
///
/// `prepare` does everything that must complete before the memo — the keys — can leave the device:
/// mint the outputs and reserve the handoff. It builds and submits nothing: the transactions come back
/// to be scheduled inside the transaction that makes the payment durable, and are built by their
/// policies afterwards.
struct PreparedStrategy {
    /// Memo entries for the coins the recipient receives — built from what `prepare` minted.
    let memoEntries: [PlannedMemoEntry]
    let handoffCommit: any CoinageHandoffCommit

    /// Still to be scheduled. Empty for a strategy that puts nothing of ours on chain.
    let transactions: [CoinageScheduledTxRequest]
}

/// Protocol for transfer execution strategies. Each strategy mints its outputs and pre-commits the
/// handoff, and declares the transactions that will spend them.
protocol TransferStrategy {
    /// Mints outputs (persisted by the allocator), submits the extrinsic(s) fire-and-forget under
    /// `groupId` (the transfer's message id, or `nil` when ungrouped), and pre-commits the handoff.
    /// Returns the memo entries and the handoff handle.
    func prepare(
        groupId: CoinageTxGroupId?,
        custodyId: String?,
        authorization: (@Sendable () throws -> Void)?
    ) async throws -> PreparedStrategy
}

extension CoinageTxServicing {
    /// Native registration commits the recipient custody before the engine can start submission.
    /// The normal transport path keeps its existing provisional handoff semantics.
    func prepareTransfer(
        requests: [CoinageTxRequest],
        coins: [Coin],
        groupId: CoinageTxGroupId?,
        custodyId: String?,
        authorization: (@Sendable () throws -> Void)?,
        afterSubmission: (() async -> Void)? = nil
    ) async throws -> PreparedStrategy {
        let memoEntries = coins.map {
            PlannedMemoEntry(coinDerivationIndex: $0.derivationIndex, valueExponent: $0.exponent)
        }
        let handoffCommit: any CoinageHandoffCommit
        if let custodyId {
            guard let authorization else { throw NativeTransferCustodyError.invalidRecord }
            try Task.checkCancellation()
            let custody = NativeTransferCustody(custodyId: custodyId, coins: coins)
            if requests.isEmpty {
                handoffCommit = try await retainNativeTransfer(custody, authorization: authorization)
            } else {
                try await submitTransactions(requests, groupId: groupId, custody: custody, authorization: authorization)
                await afterSubmission?()
                guard let retained = try await retainedNativeTransfer(custodyId: custodyId) else {
                    throw NativeTransferCustodyError.incompleteRegistration
                }
                handoffCommit = retained.handoffCommit
            }
        } else {
            if !requests.isEmpty {
                try await submitTransactions(requests, groupId: groupId)
                await afterSubmission?()
            }
            handoffCommit = try await preCommitHandoff(coins.map { .coin($0.derivationIndex, $0.publicKey) })
        }
        return PreparedStrategy(memoEntries: memoEntries, handoffCommit: handoffCommit)
    }
}
