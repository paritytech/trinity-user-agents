import Foundation
import SubstrateSdk

/// Which head a question is asked at.
public enum HeadKind: Sendable, Equatable {
    case finalized
    case best
}

/// The statuses of one domain's transactions, as read at the start of a pass.
public protocol LedgerView: Sendable {
    var transactions: [DurableTxEntry] { get }

    func status(of id: DurableTxId) -> DurableTxStatus?
}

/// A ``LedgerView`` over one snapshot of a domain's entries.
public struct SnapshotLedgerView: LedgerView {
    public let transactions: [DurableTxEntry]
    private let statusById: [DurableTxId: DurableTxStatus]

    public init(transactions: [DurableTxEntry]) {
        self.transactions = transactions
        statusById = Dictionary(transactions.map { ($0.id, $0.status) }, uniquingKeysWith: { first, _ in first })
    }

    public func status(of id: DurableTxId) -> DurableTxStatus? {
        statusById[id]
    }
}

/// A domain's reads for one pass, already issued: the ladder asks it two questions per transaction and
/// it answers without suspending, so a per-transaction chain read is not expressible.
public protocol TxCompletionPassScope: Sendable {
    /// Positive proof `transaction` took effect at `head`.
    ///
    /// Safe to assert optimistically at the best head: a reorg demotes it through Rule 0. This is also
    /// the only source of pre-finality success — the body search is bounded at the finalized head and
    /// can never establish it.
    func provenCompleted(_ transaction: DurableTxEntry, at head: HeadKind) -> Bool

    /// Positive proof `transaction` has not taken effect at `head` — and could not have taken effect and been
    /// erased since.
    ///
    /// Assert this only when the observed state is monotone and this transaction is its only writer. At
    /// the finalized head, past mortality, the engine turns it into a terminal `failure` that releases
    /// whatever the domain locked and is never revised. Leaving it false costs a block-body search over
    /// the mortality window; getting it wrong costs a double spend, which is why it defaults to false
    /// and must be opted into.
    func provenNotCompleted(_ transaction: DurableTxEntry, at head: HeadKind) -> Bool
}

public extension TxCompletionPassScope {
    func provenNotCompleted(_: DurableTxEntry, at _: HeadKind) -> Bool {
        false
    }
}

/// A domain's answer to the only two questions the ladder asks of it.
///
/// Both are positive-form. Neither firing means the reads established nothing, which is always sound:
/// the transaction falls through to the block-body search, which is ground truth. The engine never
/// negates an answer, so a transport error cannot become a verdict.
public protocol TxCompletionOracle: Sendable {
    /// The chain this domain's transactions live on.
    ///
    /// Declared per domain rather than assumed globally, so a second domain on another chain needs no
    /// change to the engine — and so a pass can pin one view per distinct chain rather than one per
    /// domain.
    var chainId: ChainId { get }

    /// Every chain read the domain needs, for every transaction in this pass, issued here.
    ///
    /// `transactions` are the ones the pass will decide; `ledger` holds every transaction of the domain
    /// with its current status, for domains whose answers depend on other transactions. A throw aborts
    /// the pass for this domain; nothing is written and the next pass repeats it.
    func openPass(
        transactions: [DurableTxEntry],
        ledger: any LedgerView,
        view: any PinnedChainViewProtocol
    ) async throws -> any TxCompletionPassScope
}

/// For a domain whose effects it cannot read — every transaction is then decided by the recorded
/// inclusion rule and the body search alone, which is correct, just slower.
///
/// The chain is still required: without it there is no view to pin and nothing could be decided at all,
/// so a domain says where it lives even when it cannot say what it sees.
public struct UnobservableOracle: TxCompletionOracle {
    public let chainId: ChainId

    public init(chainId: ChainId) {
        self.chainId = chainId
    }

    public func openPass(
        transactions _: [DurableTxEntry],
        ledger _: any LedgerView,
        view _: any PinnedChainViewProtocol
    ) async throws -> any TxCompletionPassScope {
        UnobservablePassScope()
    }
}

private struct UnobservablePassScope: TxCompletionPassScope {
    func provenCompleted(_: DurableTxEntry, at _: HeadKind) -> Bool {
        false
    }
}
