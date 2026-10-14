import Foundation

public struct ReactionViewModel: Hashable, Identifiable {
    public let id: String
    public let emoji: String
    public let count: Int
    public let isSelectedByCurrentUser: Bool

    public init(
        emoji: String,
        count: Int,
        isSelectedByCurrentUser: Bool
    ) {
        id = emoji
        self.emoji = emoji
        self.count = count
        self.isSelectedByCurrentUser = isSelectedByCurrentUser
    }
}
