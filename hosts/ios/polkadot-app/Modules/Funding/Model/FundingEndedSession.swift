import Foundation
import TrUAPIHost

/// An ended session as its detail screen draws it: its record, with the
/// core's view of how it ended, plus the quote and the steps the core
/// reported while it still holds the session. Without them the steps are the
/// direction's milestones.
struct FundingEndedSession: Equatable {
    let record: FundingRecord
    /// The core's summary, whose `ended` says how the session ended.
    let summary: FundingSessionSummary
    var quote: FundingQuote?
    var reportedSteps: [FundingProgressStep]?
}

extension FundingEndedSession {
    /// The detail of an ended session from its summary in the core's
    /// history, with the session and progress while the core holds them.
    init?(summary: FundingSessionSummary, session: FundingSession? = nil, progress: FundingProgress? = nil) {
        guard summary.ended.outcome != .open,
              let record = FundingRecord(summary: summary, network: progress?.deposit?.network)
        else { return nil }

        self.init(
            record: record,
            summary: summary,
            quote: session?.choice?.quote ?? summary.choice?.quote,
            reportedSteps: progress?.steps
        )
    }

    var direction: FundingDirection {
        record.direction
    }

    var outcome: FundingEndedOutcome {
        summary.ended.outcome
    }

    var succeeded: Bool {
        outcome == .succeeded
    }

    /// The CASH that moved, or for a session that did not go through, the
    /// CASH it was for.
    var cashAmount: U128? {
        summary.ended.cashAmount
    }

    /// What the user paid for value in, or receives for value out, in the
    /// asset of the quote the session ran on: the record's copy, or the quote
    /// while the core still holds it.
    var quotedAmount: String? {
        let quoted = quote.map { direction == .in ? $0.sendAmount : $0.receiveAmount }
        guard let units = record.paidAmount ?? quoted,
              let asset = record.paidAsset ?? record.asset,
              let decimals = record.paidDecimals ?? quote?.assetDecimals
        else { return nil }
        return FundingAssetUnit(code: asset, decimals: decimals).format(units)
    }

    /// The CASH asked for, when the CASH that moved differs from it.
    var differingRequestedAmount: U128? {
        summary.ended.differingRequested
    }

    /// The payout state of value out that the provider released.
    var payoutState: FundingPayoutState? {
        summary.ended.payoutState
    }

    var steps: [FundingStep] {
        reportedSteps?.map(\.step) ?? Self.steps(direction: direction)
    }

    /// The dot of the milestone the core says the session stopped on.
    var stoppedAt: Int? {
        summary.ended.stoppedAt.flatMap { steps.firstIndex(of: $0) }
    }

    /// The dot a released withdrawal waits on: the first milestone the
    /// provider has not reached, which is the payout.
    var waitingAt: Int? {
        guard payoutState == .pending else { return nil }
        return reportedSteps?.firstIndex { $0.reachedAtMs == nil } ?? steps.firstIndex(of: .sent)
    }

    /// The card or bank turned the payment down.
    var isDeclined: Bool {
        outcome == .failed(reason: .declined)
    }
}

private extension FundingEndedSession {
    /// The milestones the core reports for a session of this direction, for
    /// a session it no longer holds.
    static func steps(direction: FundingDirection) -> [FundingStep] {
        switch direction {
        case .out: [.started, .received, .sent]
        case .in: [.started, .received, .added]
        }
    }
}

extension FundingDeposit {
    /// The crypto network a deposit is made on.
    var network: String? {
        guard case let .crypto(_, network, _, _, _, _, _, _) = self else { return nil }
        return network
    }
}
