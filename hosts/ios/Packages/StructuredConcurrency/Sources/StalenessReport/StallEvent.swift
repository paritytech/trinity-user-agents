import Foundation

enum StallEvent {
    case activityStarted(id: UUID, title: String, at: Date)
    case regionStarted(id: UUID, parent: UUID, title: String, at: Date)
    case ended(id: UUID, at: Date, outcome: RegionOutcome)
    case dismissed(id: UUID)
    case barrier(@Sendable () -> Void)
}
