import Foundation
import TrUAPIHost

/// An ended session as its detail screen draws it: the core's summary, whose
/// `ended` says how it ended, plus the quote, the deposit's network and the
/// steps the core reported while it still holds the session. Without them
/// the steps are the direction's milestones.
struct FundingEndedSession: Equatable {
    let summary: FundingSessionSummary
    var quote: FundingQuote?
    /// The crypto network the value moved on: the one the provider named for
    /// the deposit, or the one the quote was asked for.
    var network: String?
    var reportedSteps: [FundingProgressStep]?
}

extension FundingEndedSession {
    /// The detail of an ended session from its summary in the core's
    /// history, with the session and progress while the core holds them.
    init?(summary: FundingSessionSummary, session: FundingSession? = nil, progress: FundingProgress? = nil) {
        guard summary.ended.outcome != .open else { return nil }

        self.init(
            summary: summary,
            quote: session?.choice?.quote ?? summary.choice?.quote,
            network: progress?.deposit?.network ?? summary.choice?.network,
            reportedSteps: progress?.steps
        )
    }

    var direction: FundingDirection {
        summary.direction
    }

    var rail: FundingRail? {
        summary.choice?.rail
    }

    var providerId: ProductId? {
        summary.providerId
    }

    var openedAt: Date {
        Date(milliseconds: summary.openedAtMs)
    }

    var endedAt: Date {
        Date(milliseconds: summary.ended.atMs)
    }

    /// The CASH credited or debited, once it was.
    var settledAmount: U128? {
        switch summary.stage {
        case let .delivered(credited, _): credited
        case let .released(debited, _): debited
        case .open,
             .failed: nil
        }
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
    /// asset of the quote the session ran on.
    var quotedAmount: String? {
        guard let quote, let asset = summary.choice?.asset else { return nil }
        let units = direction == .in ? quote.sendAmount : quote.receiveAmount
        return FundingAssetUnit(code: asset, decimals: quote.assetDecimals).format(units)
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
