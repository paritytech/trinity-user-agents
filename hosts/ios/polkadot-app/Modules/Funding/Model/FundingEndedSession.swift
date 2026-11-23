import Foundation
import SubstrateSdk
import TrUAPIHost

/// An ended session as its detail screen draws it: its record, plus the quote
/// and the steps the core reported when it pushed the session this run.
/// Without them the steps are rebuilt from the outcome.
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
        case .cancelled:
            return .failed(.cancelled)
        case let .failed(failure):
            return .failed(failure)
        }
    }

    var succeeded: Bool {
        outcome == .succeeded
    }

    /// The CASH that moved, or for a session that did not go through, the
    /// CASH it was for.
    var cashAmount: Balance? {
        record.settledAmount ?? record.requestedAmount ?? Balance(units: quote.map { quote in
            direction == .in ? quote.receiveAmount : quote.sendAmount
        })
    }

    /// What the user paid for value in, or receives for value out, in the
    /// asset of the quote the session ran on, with that asset's code and
    /// decimals: the record's copy, or the quote while the core still holds
    /// it.
    var quotedAmount: (amount: Balance, asset: String, decimals: UInt8)? {
        let quoted = Balance(units: quote.map { direction == .in ? $0.sendAmount : $0.receiveAmount })
        guard let amount = record.paidAmount ?? quoted,
              let asset = record.paidAsset ?? record.asset,
              let decimals = record.paidDecimals ?? quote?.assetDecimals
        else { return nil }
        return (amount, asset, decimals)
    }

    /// The CASH asked for, when the CASH that moved differs from it.
    var differingRequestedAmount: Balance? {
        guard let requested = record.requestedAmount, let settled = record.settledAmount,
              requested != settled
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
        reportedSteps?.map(\.step) ?? Self.steps(direction: direction)
    }

    /// The milestone the session stopped on: the first one a failed session
    /// did not reach, or Sent when the payout failed.
    var stoppedAt: Int? {
        switch outcome {
        case .succeeded:
            nil
        case .payoutFailed:
            steps.firstIndex(of: .sent) ?? steps.indices.last
        case .failed:
            firstUnreached ?? steps.firstIndex(of: .received)
        }
    }

    /// The milestone a released withdrawal waits on: the first one the
    /// provider has not reached, which is the payout.
    var waitingAt: Int? {
        guard payoutState == .pending else { return nil }
        return firstUnreached ?? steps.firstIndex(of: .sent)
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

    /// The milestones the core reports for a session of this direction, for
    /// a record drawn from the core's summary.
    static func steps(direction: FundingDirection) -> [FundingStep] {
        switch direction {
        case .out: [.started, .received, .sent]
        case .in: [.started, .received, .added]
        }
    }
}
