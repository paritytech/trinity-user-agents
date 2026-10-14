import Foundation

public struct StallStep: Sendable, Equatable, Identifiable {
    public enum State: Sendable, Equatable {
        case running
        case finished
        case failed(detail: String?)
        case skipped
    }

    public let id: String
    public let title: String
    public let depth: Int
    public let state: State
    public let startedAt: Date
    public let finishedAt: Date?

    public init(
        id: String,
        title: String,
        depth: Int,
        state: State,
        startedAt: Date,
        finishedAt: Date?
    ) {
        self.id = id
        self.title = title
        self.depth = depth
        self.state = state
        self.startedAt = startedAt
        self.finishedAt = finishedAt
    }
}

public struct StallActivity: Sendable, Equatable, Identifiable {
    public enum Visibility: Sendable, Equatable {
        case whenStale
        case immediate
        case never
    }

    public let id: UUID
    public let title: String
    public let subtitle: String?
    public let startedAt: Date
    public let endedAt: Date?
    public let visibility: Visibility
    public let steps: [StallStep]

    public init(
        id: UUID,
        title: String,
        subtitle: String?,
        startedAt: Date,
        endedAt: Date? = nil,
        visibility: Visibility,
        steps: [StallStep]
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.visibility = visibility
        self.steps = steps
    }
}
