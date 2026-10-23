import DurableTransactions
import Foundation

/// The memo to hand to the transport and its idempotent handoff commit handle.
///
/// Ordinary transfers reserve provisionally: commit after the carrying payload is durable, or a
/// relaunch releases the reservation. Native-custody transfers have already committed their marks
/// and derivation journal atomically before returning; their keys remain recoverable across relaunch.
public struct PreparedTransfer {
    public let memo: TransferMemo
    public let handoffCommit: any CoinageHandoffCommit

    private let transactions: [CoinageScheduledTxRequest]
    private let groupId: CoinageTxGroupId
    private let txService: any CoinageTxServicing

    init(
        memo: TransferMemo,
        handoffCommit: any CoinageHandoffCommit,
        transactions: [CoinageScheduledTxRequest],
        groupId: CoinageTxGroupId,
        txService: any CoinageTxServicing
    ) {
        self.memo = memo
        self.handoffCommit = handoffCommit
        self.transactions = transactions
        self.groupId = groupId
        self.txService = txService
    }

    /// Makes the handoff final and registers the payment's transactions, both inside `scope`.
    ///
    /// The transport opens that transaction to write its own row, so all three commit together: a saved
    /// payment always has its transactions and its reservation, and a lost one has neither.
    public func commit(in scope: any DurableTxRegistrationScope) throws {
        try handoffCommit.commit(in: scope)

        guard !transactions.isEmpty else { return }

        try txService.scheduleTransactions(transactions, groupId: groupId, joining: scope)
    }

    /// Drops the reservation for a payment whose keys never left. Nothing was scheduled, so there is
    /// nothing else to undo.
    public func abandon() async throws {
        try await handoffCommit.release()
    }
}
