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

    /// Still to be scheduled. Empty for a strategy that puts nothing of ours on chain, and for a native
    /// transfer, whose transactions were scheduled together with its custody.
    let transactions: [CoinageScheduledTxRequest]
}

/// A native-custody transfer: its recipient custody is retained atomically with its transactions, under
/// `groupId`, after `authorization` passes inside that same write.
struct NativeTransferRequest {
    let custodyId: String
    let groupId: CoinageTxGroupId?
    let authorization: @Sendable () throws -> Void
}

/// Protocol for transfer execution strategies. Each strategy mints its outputs and pre-commits the
/// handoff, and declares the transactions that will spend them.
protocol TransferStrategy {
    /// Mints outputs (persisted by the allocator) and pre-commits the handoff. Returns the memo
    /// entries, the handoff handle, and the transactions still to be scheduled.
    ///
    /// Takes no group: a strategy declares transactions but registers none, so the group they are
    /// registered under is the caller's to choose when it commits them. A `native` transfer is the
    /// exception: its transactions and custody are registered here, in one write, before the memo exists.
    func prepare(native: NativeTransferRequest?) async throws -> PreparedStrategy
}

extension CoinageTxServicing {
    /// Reserves the coins a strategy hands off, then returns what the strategy prepared.
    ///
    /// The normal transport path pre-commits a provisional handoff and leaves `transactions` for the
    /// transport to schedule. Native custody instead commits the recipient custody before the engine
    /// can start submission: an exact match retains it alone, anything else schedules its transactions
    /// in the same write.
    func prepareTransfer(
        handingOff coins: [Coin],
        memoEntries: [PlannedMemoEntry],
        transactions: [CoinageScheduledTxRequest],
        native: NativeTransferRequest?
    ) async throws -> PreparedStrategy {
        guard let native else {
            let handoffCommit = try await preCommitHandoff(coins.map { .coin($0.derivationIndex, $0.publicKey) })
            return PreparedStrategy(memoEntries: memoEntries, handoffCommit: handoffCommit, transactions: transactions)
        }

        try Task.checkCancellation()
        let custody = NativeTransferCustody(custodyId: native.custodyId, coins: coins)
        let handoffCommit: any CoinageHandoffCommit
        if transactions.isEmpty {
            handoffCommit = try await retainNativeTransfer(custody, authorization: native.authorization)
        } else {
            try await scheduleTransactions(
                transactions, groupId: native.groupId, custody: custody, authorization: native.authorization
            )
            guard let retained = try await retainedNativeTransfer(custodyId: native.custodyId) else {
                throw NativeTransferCustodyError.incompleteRegistration
            }
            handoffCommit = retained.handoffCommit
        }
        return PreparedStrategy(memoEntries: memoEntries, handoffCommit: handoffCommit, transactions: [])
    }
}
