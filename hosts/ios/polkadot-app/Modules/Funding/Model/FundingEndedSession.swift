import Foundation
import TrUAPIHost

/// An ended session as its detail screen draws it: the host's record of it,
/// plus the quote and the steps the core reported while it still holds the
/// session. Without them the steps are rebuilt from the outcome.
struct FundingEndedSession: Equatable {
    enum Outcome: Equatable {
        case succeeded
        case failed(FundingFailure)
        case payoutFailed(reason: String)
    }

    let record: FundingRecord
    var quote: FundingQuote?
    var reportedSteps: [FundingProgressStep]?
}

extension FundingEndedSession {
    init?(session: FundingSession, progress: FundingProgress?) {
        guard let record = FundingRecord(session: session, progress: progress) else { return nil }
        self.init(record: record, quote: session.choice?.quote, reportedSteps: progress?.steps)
    }

    var direction: FundingDirection {
        record.direction
    }

    var outcome: Outcome {
        switch record.outcome {
        case .delivered:
            return .succeeded
        case .released:
            if case let .failed(reason) = record.payout { return .payoutFailed(reason: reason) }
            return .succeeded
        case .refunded:
            return .failed(.refunded)
        case let .failed(failure):
            return .failed(failure)
        }
    }

    var succeeded: Bool {
        outcome == .succeeded
    }

    /// The CASH that moved, or for a session that did not go through, the
    /// CASH it was for.
    var cashAmount: U128? {
        record.settledAmount ?? record.requestedAmount ?? quote.map { quote in
            direction == .in ? quote.receiveAmount : quote.sendAmount
        }
    }

    /// What the user paid for value in, or receives for value out, in the
    /// asset of the quote the session ran on.
    var quotedAmount: String? {
        guard let quote, let asset = record.asset else { return nil }
        return FundingAssetUnit(code: asset).format(direction == .in ? quote.sendAmount : quote.receiveAmount)
    }

    var steps: [FundingStep] {
        reportedSteps?.map(\.step) ?? Self.steps(direction: direction, rail: record.rail)
    }

    /// The step the session stopped on: the first one a failed session did
    /// not reach, or the last one when the payout failed.
    var stoppedAt: Int? {
        switch outcome {
        case .succeeded:
            nil
        case .payoutFailed:
            steps.indices.last
        case .failed:
            reportedSteps?.firstIndex { $0.reachedAtMs == nil } ?? steps.firstIndex(of: .payment)
        }
    }

    /// The provider said the card or bank turned the payment down. The core
    /// has no failure of its own for it, so it arrives as `Other`.
    var isDeclined: Bool {
        guard case let .failed(.other(code, _)) = outcome else { return false }
        return code.lowercased().contains("declin")
    }
}

private extension FundingEndedSession {
    /// The steps the core reports for a session of this direction and rail.
    static func steps(direction: FundingDirection, rail: FundingRail?) -> [FundingStep] {
        switch (direction, rail) {
        case (.out, _):
            [.started, .payment, .sent]
        case (.in, .card),
             (.in, nil):
            [.started, .payment, .conversion, .added]
        case (.in, .bank),
             (.in, .crypto):
            [.started, .payment, .approved, .conversion, .added]
        }
    }
}
