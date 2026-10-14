import AsyncExtensions
import Foundation

/// Provides the app badge count from persisted incoming-new chat messages.
protocol UnreadMessageCountServicing {
    func totalUnreadBadgeMessageCount() async throws -> Int
    func totalUnreadBadgeMessageCountStream() -> AnyAsyncSequence<Int>
}

extension UnreadMessageCountService: UnreadMessageCountServicing {
    func totalUnreadBadgeMessageCountStream() -> AnyAsyncSequence<Int> {
        ChatMessageDataProviderFactory()
            .subscribeMessages(with: Self.badgeCountPredicate())
            .map { messages in
                Set(messages.map(\.groupingId)).count
            }
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }
}
