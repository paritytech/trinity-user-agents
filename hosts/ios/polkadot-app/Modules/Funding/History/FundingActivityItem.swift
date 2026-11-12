import Foundation
import FoundationExt
import SubstrateSdk
import TrUAPIHost

/// One row of the CASH card's funding lists.
struct FundingActivityItem: Identifiable, Equatable {
    enum Status: Equatable {
        /// In flight, where the core says it stands.
        case inProgress(FundingStatus)
        case toppedUp
        case sent
        case refunded
        /// The payout failed after the balance was debited; `refunded` is what
        /// the provider's refund returned, once final.
        case payoutFailed(refunded: U128?)
        /// Cancelled at the user's request; `refunded` is what came back when
        /// money had moved.
        case cancelled(refunded: U128?)
        case failed(FundingFailure)
    }

    let id: String
    let direction: FundingDirection
    let rail: FundingRail?
    /// The CASH it is for, in planks; nil when the core names none or sends
    /// one that is not a number.
    let amount: Balance?
    let date: Date
    let status: Status
}

extension FundingActivityItem {
    init(session: FundingSession, progress: FundingProgress?) {
        id = session.intent
        direction = session.direction
        rail = session.choice?.rail
        let units = session.amount ?? session.choice.map { choice in
            session.direction == .in ? choice.quote.receiveAmount : choice.quote.sendAmount
        }
        amount = units.flatMap { Balance($0) }
        date = Date(milliseconds: session.openedAtMs)
        status = .inProgress(progress?
            .status ?? (session.direction == .in ? .waitingForPayment : .transactionInitiated))
    }

    /// The row for an ended session from its summary in the core's history;
    /// nil while it is open.
    init?(summary: FundingSessionSummary) {
        guard let status = Status(ended: summary) else { return nil }

        id = summary.intent
        direction = summary.direction
        rail = summary.choice?.rail
        amount = summary.ended.cashAmount.flatMap { Balance($0) }
        date = Date(milliseconds: summary.ended.atMs)
        self.status = status
    }

    var isInProgress: Bool {
        if case .inProgress = status { return true }
        return false
    }

    /// In flight but slower than it should be, which the row draws in amber.
    var isDelayed: Bool {
        guard case let .inProgress(status) = status else { return false }
        return status.isDelayed
    }
}

private extension FundingActivityItem.Status {
    /// How the core says `summary` ended; a cancel reads as cancelled, with
    /// what came back when money had moved.
    init?(ended summary: FundingSessionSummary) {
        switch summary.ended.outcome {
        case .open:
            return nil
        case .succeeded:
            self = summary.direction == .in ? .toppedUp : .sent
        case .payoutFailed:
            self = .payoutFailed(refunded: summary.refunded)
        case .failed(.cancelled):
            self = .cancelled(refunded: summary.refunded)
        case .failed(.refunded):
            self = .refunded
        case let .failed(reason):
            self = .failed(reason)
        }
    }
}

extension FundingStatus {
    /// Still moving, as the core says: the CASH card lists it under In
    /// progress, and the host keeps it out of its own history until it is not.
    var isInFlight: Bool {
        fundingStatusInFlight(status: self)
    }

    /// Slower than it should be, which the row draws in amber.
    var isDelayed: Bool {
        switch self {
        case .retrying,
             .retryingPayout,
             .takingLonger,
             .cancelOffered,
             .cancelRefused: true
        default: false
        }
    }
}

extension Date {
    init(milliseconds: UInt64) {
        self.init(timeIntervalSince1970: milliseconds.millisecondsToSeconds())
    }
}

extension FundingActivityItem {
    /// The sessions in flight, newest first, and the ended ones, most
    /// recently ended first.
    static func lists(from entries: [FundingEntry])
        -> (inFlight: [FundingActivityItem], history: [FundingActivityItem]) {
        let inFlight = entries
            .filter { $0.progress?.status.isInFlight == true }
            .compactMap { entry in entry.session.map { FundingActivityItem(session: $0, progress: entry.progress) } }
            .sorted { $0.date > $1.date }
        let inFlightIds = Set(inFlight.map(\.id))
        let history = entries
            .filter { !inFlightIds.contains($0.id) }
            .compactMap { $0.summary.flatMap(FundingActivityItem.init(summary:)) }
            .sorted { $0.date > $1.date }
        return (inFlight, history)
    }
}
