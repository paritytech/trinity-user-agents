import Foundation

public struct OutgoingStatementRenewalSummary: Sendable, Equatable {
    public let isComplete: Bool
    public let candidates: Int
    public let due: Int
    public let republished: Int
    public let failed: Int
    public let statementsWithMissedPeriods: Int
    public let missedPeriods: Int

    public init(
        isComplete: Bool,
        candidates: Int,
        due: Int,
        republished: Int,
        failed: Int,
        statementsWithMissedPeriods: Int,
        missedPeriods: Int
    ) {
        self.isComplete = isComplete
        self.candidates = candidates
        self.due = due
        self.republished = republished
        self.failed = failed
        self.statementsWithMissedPeriods = statementsWithMissedPeriods
        self.missedPeriods = missedPeriods
    }

    public static let incomplete = OutgoingStatementRenewalSummary(
        isComplete: false,
        candidates: 0,
        due: 0,
        republished: 0,
        failed: 0,
        statementsWithMissedPeriods: 0,
        missedPeriods: 0
    )
}
