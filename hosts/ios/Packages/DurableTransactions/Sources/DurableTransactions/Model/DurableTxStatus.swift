import Foundation

/// Lifecycle status of a durable transaction.
///
/// `pending` and `pendingSuccess` are live — a recovery pass keeps evaluating them.
/// `finalizedSuccess` and `failure` are terminal and are never rewritten.
/// `pendingSubmission` is live but carries no attempt: nothing is on the wire for it.
public enum DurableTxStatus: Int, Sendable, Equatable, CaseIterable {
    case pending = 0
    case pendingSuccess = 1
    case finalizedSuccess = 2
    case failure = 3

    /// Nothing is in flight: the transaction waits for its ``DurableSubmissionPolicy`` to build it,
    /// either for the first time or again after an attempt that can never land.
    case pendingSubmission = 4
}

public extension DurableTxStatus {
    /// Live transactions hold whatever their domain locked for them.
    var isLive: Bool {
        self == .pending || self == .pendingSuccess || self == .pendingSubmission
    }

    /// Bytes were submitted and nothing has concluded about them, so a recovery pass may decide it.
    ///
    /// Deliberately narrower than ``isLive``: a transaction waiting to be built holds its domain's
    /// locks but has no attempt for a pass to read, so no rule could ever decide it.
    var awaitsVerdict: Bool {
        self == .pending || self == .pendingSuccess
    }

    /// Executed in a block, finalized or not.
    ///
    /// The threshold a domain reads on-chain presence against: an effect is only absent-because-consumed
    /// if whatever produced it actually ran, and asking for finality there while presence is read at the
    /// best head reports something that plainly existed a moment ago as something that may never have.
    var isArrived: Bool {
        self == .pendingSuccess || self == .finalizedSuccess
    }

    /// Whether there is still a way for this transaction to complete — already or in the future. The only
    /// status that provably never completes is the terminal `failure`.
    var canArrive: Bool {
        self != .failure
    }
}
