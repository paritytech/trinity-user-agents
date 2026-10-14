import Foundation
import SDKLogger

/// Decides one transaction against one pinned view. Evaluated in order; the first rule that matches wins.
///
/// Everything domain-shaped reaches the ladder through the oracle's scope, asked per head: completion
/// at F and at B are Rules 1 and 2, non-completion at F and at B are Rules 3 and 4, and the body search
/// is the ground truth underneath both. `view` is consulted only by the last rule.
///
/// `recordedStillCanonical` is resolved by the pass, batched across every transaction that carries a
/// record, so Rule 0 costs no read of its own here — `nil` is a read that failed, which leaves the
/// transaction undecided.
public struct CompletionLadder: Sendable {
    private let logger: SDKLoggerProtocol?

    public init(logger: SDKLoggerProtocol? = nil) {
        self.logger = logger
    }

    public func evaluate(
        _ transaction: DurableTxEntry,
        scope: any TxCompletionPassScope,
        view: any PinnedChainViewProtocol,
        recordedStillCanonical: Bool?
    ) async -> RuleOutcome {
        // A transaction with no attempt has no bytes, no window and no inclusion to read, so no rule
        // could decide it. Such rows never reach a pass; this keeps the type honest rather than
        // inventing a window for one that has none.
        guard let attempt = transaction.attempt else {
            return .undecided
        }

        if let outcome = recordedInclusion(
            transaction,
            scope: scope,
            view: view,
            stillCanonical: recordedStillCanonical
        ) {
            return outcome
        }

        // Rule 1 — completion is visible at the finalized head.
        if scope.provenCompleted(transaction, at: .finalized) {
            return decided(transaction, rule: "1 completed at F", .finalizedSuccess, at: transaction.successDetectedAt)
        }

        // Rule 2 — completion is visible at the best head. Rule 1 is evaluated first and wins on the
        // same evidence, so the overlap only ever costs a weaker verdict.
        if scope.provenCompleted(transaction, at: .best) {
            return decided(transaction, rule: "2 completed at B", .pendingSuccess, at: view.bestHead)
        }

        let windowClosed = attempt.isWindowClosed(atFinalized: view.finalizedHead.number)

        // Rule 3 — proven not to have run, and it can no longer run.
        if windowClosed, scope.provenNotCompleted(transaction, at: .finalized) {
            return decided(transaction, rule: "3 not completed at F", .failure, at: nil, failure: .expired)
        }

        // Rule 4 — short-circuits, so a transaction with no positive evidence does not run a body search
        // on every new head. It must not fire once mortality has expired: past it the transaction has to
        // reach the search, which is the only thing left that can decide it.
        if !windowClosed, scope.provenNotCompleted(transaction, at: .best) {
            return decided(transaction, rule: "4 not completed at B", .pending, at: nil)
        }

        return await searchForTransaction(
            transaction,
            attempt: attempt,
            view: view,
            windowClosed: windowClosed
        )
    }
}

// MARK: - Rule 0

private extension CompletionLadder {
    /// We already saw this transaction included somewhere; check that block is still real.
    ///
    /// The record is only ever written where completion is already proven, so this never re-asks whether
    /// the transaction took effect. Returns `nil` when no success block is recorded, letting evaluation
    /// fall through to Rule 1.
    func recordedInclusion(
        _ transaction: DurableTxEntry,
        scope: any TxCompletionPassScope,
        view: any PinnedChainViewProtocol,
        stillCanonical: Bool?
    ) -> RuleOutcome? {
        guard let recorded = transaction.successDetectedAt else { return nil }

        guard let stillCanonical else {
            logger?
                .warning("\(transaction.id) rule=undecided reason=record-canonicality-unread record=\(recorded.number)")
            return .undecided
        }

        if !stillCanonical {
            // Asked before the best head, or a chain that reorgs its head between passes would keep
            // re-recording this transaction above the finalized head and never let it finalize at all.
            if scope.provenCompleted(transaction, at: .finalized) {
                return decided(
                    transaction,
                    rule: "0 record gone, completed at F",
                    .finalizedSuccess,
                    at: view.finalizedHead
                )
            }
            if scope.provenCompleted(transaction, at: .best) {
                return decided(transaction, rule: "0 record gone, still at B", .pendingSuccess, at: view.bestHead)
            }
            // Writes pending rather than only clearing the record: clearing alone would leave the
            // transaction pendingSuccess with no evidence behind it, and whatever its effect made
            // selectable would stay so for a full mortality window on the strength of a vanished block.
            return decided(transaction, rule: "0 record gone, demoted", .pending, at: nil)
        }

        return recorded.number <= view.finalizedHead.number
            ? decided(transaction, rule: "0 record canonical at F", .finalizedSuccess, at: recorded)
            : decided(transaction, rule: "0 record canonical above F", .pendingSuccess, at: recorded)
    }
}

// MARK: - Rule 5

private extension CompletionLadder {
    /// Nothing above could decide it, so look for the transaction itself. The window ends at the
    /// finalized head, so both terminal verdicts rest on a finalized fact.
    func searchForTransaction(
        _ transaction: DurableTxEntry,
        attempt: DurableTxAttempt,
        view: any PinnedChainViewProtocol,
        windowClosed: Bool
    ) async -> RuleOutcome {
        // The checkpoint is above the finalized head, so there is nothing to read yet. The window cannot
        // be closed here: closing needs the finalized head past the mortality end, which is at or above
        // the checkpoint.
        guard let window = searchWindow(attempt, finalizedNumber: view.finalizedHead.number) else {
            return decided(transaction, rule: "5 nothing to search yet", .pending, at: nil)
        }

        switch await view.searchBodies(for: attempt.txHash, in: window) {
        case let .foundSucceeded(block):
            return decided(transaction, rule: "5 found, dispatch succeeded", .finalizedSuccess, at: block)
        case let .foundFailed(_, reason):
            // Inclusion is not success — an extrinsic can be applied and its dispatch still fail.
            return decided(
                transaction,
                rule: "5 found, dispatch failed: \(reason ?? "unknown error")",
                .failure,
                at: nil,
                failure: .dispatchFailed
            )
        case .foundOutcomeUnreadable:
            return decided(transaction, rule: "5 found, outcome unreadable", .pending, at: nil)
        case .notFoundWindowComplete:
            return windowClosed
                ? decided(transaction, rule: "5 whole window read, absent", .failure, at: nil, failure: .expired)
                : decided(transaction, rule: "5 absent, window open", .pending, at: nil)
        case .incomplete:
            return decided(transaction, rule: "5 window incomplete", .pending, at: nil)
        }
    }

    func searchWindow(_ attempt: DurableTxAttempt, finalizedNumber: UInt32) -> ClosedRange<UInt32>? {
        let lowerBound = attempt.checkpoint.number
        let upperBound = UInt32(min(attempt.mortalityEnd, UInt64(finalizedNumber)))
        return lowerBound <= upperBound ? lowerBound ... upperBound : nil
    }
}

// MARK: - Logging

private extension CompletionLadder {
    /// Every terminal path names itself, so a log line says which rule spoke and not merely what it
    /// concluded.
    func decided(
        _ transaction: DurableTxEntry,
        rule: String,
        _ status: DurableTxStatus,
        at successDetectedAt: BlockRef?,
        failure: DurableFailureKind? = nil
    ) -> RuleOutcome {
        logger?
            .debug(
                "\(transaction.id) rule=\"\(rule)\" -> \(status) record=\(successDetectedAt?.number.description ?? "none")"
            )
        return .decided(Verdict(status: status, successDetectedAt: successDetectedAt, failure: failure))
    }
}
