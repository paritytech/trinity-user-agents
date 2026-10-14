import Foundation
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
        case failed(FundingFailure)
    }

    let id: String
    let direction: FundingDirection
    let rail: FundingRail?
    let amount: U128?
    let date: Date
    let status: Status
}

extension FundingActivityItem {
    init(session: FundingSession, progress: FundingProgress?) {
        id = session.intent
        direction = session.direction
        rail = session.choice?.rail
        amount = session.amount ?? session.choice.map { choice in
            session.direction == .in ? choice.quote.receiveAmount : choice.quote.sendAmount
        }
        date = Date(milliseconds: session.openedAtMs)
        status = .inProgress(progress?
            .status ?? (session.direction == .in ? .waitingForPayment : .transactionInitiated))
    }

    init(record: FundingRecord) {
        id = record.intent
        direction = record.direction
        rail = record.rail
        amount = record.settledAmount ?? record.requestedAmount
        date = record.settledAt

        switch record.outcome {
        case .delivered:
            status = .toppedUp
        case .released:
            if case .failed = record.payout {
                status = .payoutFailed(refunded: record.refundedAmount)
            } else {
                status = .sent
            }
        case .refunded:
            status = .refunded
        case let .failed(failure):
            status = .failed(failure)
        }
    }

    var isInProgress: Bool {
        if case .inProgress = status { return true }
        return false
    }
}

extension FundingStatus {
    /// Still moving: the CASH card lists it under In progress, and the host
    /// keeps it out of its own history until it is not.
    var isInFlight: Bool {
        switch self {
        case .waitingForTransfer,
             .waitingForPayment,
             .waitingForDeposit,
             .convertingToCash,
             .transactionInitiated,
             .convertingTo,
             .preparingPayout,
             .retrying,
             .retryingPayout,
             .takingLonger:
            true
        case .toppedUp,
             .sent,
             .refunded,
             .payoutFailed,
             .failed:
            false
        }
    }

    /// Slower than it should be, which the row draws in amber.
    var isDelayed: Bool {
        switch self {
        case .retrying,
             .retryingPayout,
             .takingLonger:
            true
        default:
            false
        }
    }
}
