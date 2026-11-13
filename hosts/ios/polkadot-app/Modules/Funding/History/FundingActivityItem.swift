import Foundation
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
        case payoutFailed(refunded: Balance?)
        /// Cancelled at the user's request; `refunded` is what came back when
        /// money had moved.
        case cancelled(refunded: Balance?)
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

    init(record: FundingRecord) {
        id = record.intent
        direction = record.direction
        rail = record.rail
        amount = (record.settledAmount ?? record.requestedAmount).flatMap { Balance($0) }
        date = record.settledAt
        status = record.outcome.activityStatus(
            payout: record.payout,
            refunded: record.refundedAmount.flatMap { Balance($0) }
        )
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

private extension FundingRecord.Outcome {
    /// The row's status for a record that ended this way; a release whose
    /// payout failed reads as a failed payout.
    func activityStatus(payout: FundingRecord.Payout?, refunded: Balance?) -> FundingActivityItem.Status {
        switch self {
        case .delivered:
            .toppedUp
        case .released:
            if case .failed = payout {
                .payoutFailed(refunded: refunded)
            } else {
                .sent
            }
        case .refunded:
            .refunded
        case .cancelled,
             .failed(.cancelled):
            .cancelled(refunded: refunded)
        case let .failed(failure):
            .failed(failure)
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
