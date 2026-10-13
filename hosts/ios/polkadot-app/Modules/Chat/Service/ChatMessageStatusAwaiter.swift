import Foundation

enum ChatMessageStatusAwaiter {
    static func waitUntilOutgoingStatusReached(
        messageId: Chat.MessageId,
        target: Chat.LocalMessage.Status.OutgoingStatus,
        providerFactory: ChatMessageDataProviderMaking,
        workQueue: DispatchQueue
    ) async throws {
        let predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            NSPredicate.chatMessage(with: messageId),
            makeStatusReachedPredicate(target: target)
        ])

        let stream = AsyncStream<Void> { continuation in
            let subscription = providerFactory.subscribeMessagesSnapshot(
                with: predicate,
                deliverOn: workQueue
            ) { messages in
                guard !messages.isEmpty else { return }
                continuation.yield(())
                continuation.finish()
            }

            continuation.onTermination = { @Sendable _ in
                _ = subscription
            }
        }

        for await _ in stream {
            return
        }

        throw CancellationError()
    }
}

private extension ChatMessageStatusAwaiter {
    static func makeStatusReachedPredicate(
        target: Chat.LocalMessage.Status.OutgoingStatus
    ) -> NSPredicate {
        let accepted: [Chat.LocalMessage.Status.OutgoingStatus] =
            switch target {
            case .new: [.new, .sent, .delivered]
            case .sent: [.sent, .delivered]
            case .delivered: [.delivered]
            case .failed: [.failed]
            }

        let subpredicates = accepted.map { NSPredicate.byStatus(.outgoing($0)) }

        return NSCompoundPredicate(orPredicateWithSubpredicates: subpredicates)
    }
}
