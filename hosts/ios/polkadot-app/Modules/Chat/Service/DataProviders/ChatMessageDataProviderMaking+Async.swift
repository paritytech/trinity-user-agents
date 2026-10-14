import Foundation
import AsyncExtensions
import Operation_iOS
import Foundation_iOS
import StructuredConcurrency

extension ChatMessageDataProviderMaking {
    func subscribeChatMessages(_ chat: Chat.Id) -> AnyAsyncSequence<[Chat.LocalMessage]> {
        subscribeMessages(with: .localMessages(from: chat))
    }

    func subscribeNewOutgoingChatRequests() -> AnyAsyncSequence<[Chat.LocalMessage]> {
        subscribeMessages(with: .newOutgoingChatRequestMessages())
    }

    func subscribeMessages(with predicate: NSPredicate?) -> AnyAsyncSequence<[Chat.LocalMessage]> {
        let syncQueue = DispatchQueue(label: "io.chat.messages.provider.async.updates")

        return AsyncStream { continuation in
            let holder = AnyObjectHolder<AnyObject>()

            let provider = subscribeMessagesSnapshot(
                with: predicate,
                deliverOn: syncQueue
            ) { messages in
                continuation.yield(messages)
            }

            holder.set(provider)

            continuation.onTermination = { @Sendable _ in
                holder.set(nil)
            }
        }
        .eraseToAnyAsyncSequence()
    }
}
