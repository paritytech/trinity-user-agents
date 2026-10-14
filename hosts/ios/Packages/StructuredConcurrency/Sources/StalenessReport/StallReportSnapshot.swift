import Foundation

public struct StallReportSnapshot: Sendable, Equatable {
    public struct Step: Sendable, Equatable, Identifiable {
        public let id: String
        public let title: String
        public let depth: Int
        public let state: StallStep.State
        public let elapsed: TimeInterval

        public init(id: String, title: String, depth: Int, state: StallStep.State, elapsed: TimeInterval) {
            self.id = id
            self.title = title
            self.depth = depth
            self.state = state
            self.elapsed = elapsed
        }
    }

    public struct Activity: Sendable, Equatable, Identifiable {
        public let id: UUID
        public let title: String
        public let subtitle: String?
        public let elapsed: TimeInterval
        public let steps: [Step]

        public init(id: UUID, title: String, subtitle: String?, elapsed: TimeInterval, steps: [Step]) {
            self.id = id
            self.title = title
            self.subtitle = subtitle
            self.elapsed = elapsed
            self.steps = steps
        }
    }

    public let activities: [Activity]

    public init(activities: [Activity]) {
        self.activities = activities
    }
}
