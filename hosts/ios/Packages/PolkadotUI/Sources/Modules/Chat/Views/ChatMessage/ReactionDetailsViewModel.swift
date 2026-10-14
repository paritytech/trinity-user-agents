import Foundation

public struct ReactionDetailsViewModel: Equatable {
    public let totalCount: Int
    public let reactions: [ReactionGroup]

    public init(totalCount: Int, reactions: [ReactionGroup]) {
        self.totalCount = totalCount
        self.reactions = reactions
    }
}

public extension ReactionDetailsViewModel {
    struct ReactionGroup: Equatable, Identifiable {
        public var id: String { emoji }
        public let emoji: String
        public let count: Int
        public let reactors: [Reactor]

        public init(emoji: String, count: Int, reactors: [Reactor]) {
            self.emoji = emoji
            self.count = count
            self.reactors = reactors
        }
    }

    struct Reactor: Equatable, Identifiable {
        public let id: String
        public let username: String
        public let timestamp: Date

        public init(id: String, username: String, timestamp: Date) {
            self.id = id
            self.username = username
            self.timestamp = timestamp
        }
    }
}
