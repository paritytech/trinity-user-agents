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

    /// Where a released outbound session's payout stands.
    enum PayoutState: Equatable {
        case pending
        case paidOut
        case failed
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
    /// asset of the quote the session ran on: the record's copy, or the quote
    /// while the core still holds it.
    var quotedAmount: String? {
        let quoted = quote.map { direction == .in ? $0.sendAmount : $0.receiveAmount }
        guard let units = record.paidAmount ?? quoted, let asset = record.paidAsset ?? record.asset else { return nil }
        return FundingAssetUnit(code: asset).format(units)
    }

    /// The CASH asked for, when the CASH that moved differs from it.
    var differingRequestedAmount: U128? {
        guard let requested = record.requestedAmount, let settled = record.settledAmount,
              Decimal(string: requested) != Decimal(string: settled)
        else { return nil }
        return requested
    }

    /// The payout state of value out that the provider released.
    var payoutState: PayoutState? {
        guard direction == .out, record.outcome == .released else { return nil }
        switch record.payout {
        case .paidOut: return .paidOut
        case .failed: return .failed
        case nil: return .pending
        }
    }

    var steps: [FundingStep] {
        reportedSteps?.map(\.step) ?? Self.steps(direction: direction, rail: record.rail)
    }

    /// The step the session stopped on: the first one a failed session did
    /// not reach, or the payout when it failed.
    var stoppedAt: Int? {
        switch outcome {
        case .succeeded:
            nil
        case .payoutFailed:
            steps.firstIndex(of: .payout) ?? steps.indices.last
        case .failed:
            firstUnreached ?? steps.firstIndex(of: .payment)
        }
    }

    /// The step a released withdrawal waits on: the first one the provider
    /// has not reported, which is the payout once it has converted.
    var waitingAt: Int? {
        guard payoutState == .pending else { return nil }
        return firstUnreached ?? steps.firstIndex(of: .payout)
    }

    /// The card or bank turned the payment down.
    var isDeclined: Bool {
        outcome == .failed(.declined)
    }
}

private extension FundingEndedSession {
    var firstUnreached: Int? {
        reportedSteps?.firstIndex { $0.reachedAtMs == nil }
    }

    /// The steps the core reports for a session of this direction and rail,
    /// for a record the core has dropped.
    static func steps(direction: FundingDirection, rail: FundingRail?) -> [FundingStep] {
        switch (direction, rail) {
        case (.out, _):
            [.started, .payment, .sent, .conversion, .payout]
        case (.in, .bank):
            [.started, .payment, .added]
        case (.in, .card),
             (.in, .crypto),
             (.in, nil):
            [.started, .payment, .approved, .conversion, .added]
        }
    }
}
