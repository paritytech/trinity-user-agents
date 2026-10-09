import DurableTransactions
import Foundation

/// The result of preparing a transfer: the memo to hand to the transport, and the two things that must
/// become durable at exactly the moment that memo does — the handoff, and the payment's transactions.
///
/// Nothing has been built or submitted yet. ``commit(in:)`` runs inside the transaction that writes
/// whatever carries the keys, so a crash either keeps all of it or none of it; the transactions are
/// built by their policies once that transaction commits. A payment whose keys never left calls
/// ``abandon()`` instead, and the coins are free again immediately rather than on the next launch.
///
/// Native-custody transfers have already committed their marks, their transactions and their derivation
/// journal atomically before returning; their keys remain recoverable across relaunch, and committing
/// again schedules nothing.
public struct PreparedTransfer {
    public let memo: TransferMemo
    public let handoffCommit: any CoinageHandoffCommit

    /// Still to be scheduled when the transport commits; `nil` once they are already durable.
    private let pending: (transactions: [CoinageScheduledTxRequest], groupId: CoinageTxGroupId, txService: any CoinageTxServicing)?

    init(
        memo: TransferMemo,
        handoffCommit: any CoinageHandoffCommit,
        transactions: [CoinageScheduledTxRequest],
        groupId: CoinageTxGroupId,
        txService: any CoinageTxServicing
    ) {
        self.memo = memo
        self.handoffCommit = handoffCommit
        pending = (transactions, groupId, txService)
    }

    /// A native-custody transfer, whose handoff and transactions were registered with its custody record.
    init(memo: TransferMemo, retaining handoffCommit: any CoinageHandoffCommit) {
        self.memo = memo
        self.handoffCommit = handoffCommit
        pending = nil
    }

    /// Makes the handoff final and registers the payment's transactions, both inside `scope`.
    ///
    /// The transport opens that transaction to write its own row, so all three commit together: a saved
    /// payment always has its transactions and its reservation, and a lost one has neither.
    public func commit(in scope: any DurableTxRegistrationScope) throws {
        try handoffCommit.commit(in: scope)

        guard let pending, !pending.transactions.isEmpty else { return }

        try pending.txService.scheduleTransactions(pending.transactions, groupId: pending.groupId, joining: scope)
    }

    /// Drops the reservation for a payment whose keys never left. Nothing was scheduled, so there is
    /// nothing else to undo.
    public func abandon() async throws {
        try await handoffCommit.release()
    }
}
