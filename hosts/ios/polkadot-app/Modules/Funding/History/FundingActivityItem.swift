import Foundation
import TrUAPIHost

/// One row of the CASH card's funding lists.
struct FundingActivityItem: Identifiable, Equatable {
    enum Status: Equatable {
        /// In flight, with where it has got to.
        case inProgress(FundingActivityStep, isDelayed: Bool)
        case toppedUp
        case sent
        case refunded
        case payoutFailed
        case failed(FundingFailure)
    }

    let id: String
    let direction: FundingDirection
    let rail: FundingRail?
    let amount: U128?
    let date: Date
    let status: Status
}

/// Where a session in flight stands, as its subtitle says it.
enum FundingActivityStep: Equatable {
    case upcoming
    case waitingForTransfer
    case converting
    case retrying
    case transactionInitiated
    case convertingOut(asset: String)
}

extension FundingActivityItem {
    init(session: FundingSession, progress: FundingProgress?, now: Date = Date()) {
        id = session.intent
        direction = session.direction
        rail = session.choice?.rail
        amount = session.amount ?? session.choice.map { choice in
            session.direction == .in ? choice.quote.receiveAmount : choice.quote.sendAmount
        }
        date = Date(milliseconds: session.openedAtMs)

        let step = Self.step(session: session, progress: progress)
        status = .inProgress(step, isDelayed: step != .retrying && Self.isDelayed(session, progress, now: now))
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
                status = .payoutFailed
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

private extension FundingActivityItem {
    static func reached(_ progress: FundingProgress?) -> Set<FundingStep> {
        Set(progress?.steps.filter { $0.reachedAtMs != nil }.map(\.step) ?? [])
    }

    static func step(session: FundingSession, progress: FundingProgress?) -> FundingActivityStep {
        if progress?.retrying == true { return .retrying }

        let reached = reached(progress)
        switch session.direction {
        case .in:
            if reached.contains(.conversion) { return .converting }
            let awaitsTransfer = session.choice.map { $0.rail != .card } ?? false
            if awaitsTransfer, !reached.contains(.payment) { return .waitingForTransfer }
            return .upcoming
        case .out:
            if reached.contains(.conversion), let asset = session.choice?.asset {
                return .convertingOut(asset: asset)
            }
            return .transactionInitiated
        }
    }

    /// Slower than the provider said: the latest step is older than the time
    /// the chosen quote gave for the whole session.
    static func isDelayed(_ session: FundingSession, _ progress: FundingProgress?, now: Date) -> Bool {
        guard let eta = session.choice?.quote.etaSecs else { return false }

        let latest = progress?.steps.compactMap(\.reachedAtMs).max() ?? session.openedAtMs
        return now.timeIntervalSince(Date(milliseconds: latest)) > TimeInterval(eta)
    }
}
