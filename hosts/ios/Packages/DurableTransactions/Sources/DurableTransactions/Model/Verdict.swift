import Foundation

/// A transaction's status write, applied atomically by
/// ``DurableTxRepositoryProtocol/updateTxStatus(for:expectedCurrentStatus:verdict:)``.
///
/// The status to write and the block where execution was observed (`nil` clears it). A verdict that
/// keeps the existing record simply re-states the same block, so the write is skipped only when both
/// `status` and `successDetectedAt` already match the entry.
public struct Verdict: Sendable, Equatable {
    public let status: DurableTxStatus
    public let successDetectedAt: BlockRef?

    /// Why a ``DurableTxStatus/failure`` was reached; `nil` for every other status.
    ///
    /// Carried so ``DurableVerdictWriter`` can offer the failure back to the transaction's policy
    /// before it becomes terminal — what makes a rebuild possible is knowing *how* the attempt died.
    public let failure: DurableFailureKind?

    public init(
        status: DurableTxStatus,
        successDetectedAt: BlockRef?,
        failure: DurableFailureKind? = nil
    ) {
        self.status = status
        self.successDetectedAt = successDetectedAt
        self.failure = failure
    }
}

/// What the ladder decided for one transaction this pass.
public enum RuleOutcome: Sendable, Equatable {
    /// Write this verdict (a compare-and-set against the status the rules were evaluated from).
    case decided(Verdict)
    /// A read this transaction depended on failed. It keeps its status and its locks, and is retried.
    case undecided
}

public extension RuleOutcome {
    var verdict: Verdict? {
        if case let .decided(verdict) = self { verdict } else { nil }
    }
}
