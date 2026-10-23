import Foundation
import AsyncExtensions
import Foundation_iOS
import StructuredConcurrency

extension ChatContactDataProviderMaking {
    func subscribeAllChats() -> AnyAsyncSequence<[Chat.LocalModel]> {
        subscribeChatsWithPredicate(nil)
    }

    func subscribeChat(by chatId: Chat.Id) -> AnyAsyncSequence<Chat.LocalModel?> {
        subscribeChatsWithPredicate(
            .chat(for: chatId.rawRepresentation)
        )
        .map { chats in
            chats.first
        }
        .eraseToAnyAsyncSequence()
    }

    func subscribeChatsWithPredicate(
        _ predicate: NSPredicate?
    ) -> AnyAsyncSequence<[Chat.LocalModel]> {
        let syncQueue = DispatchQueue(label: "io.chats.provider.async.updates")

        return AsyncThrowingStream { continuation in
            let holder = AnyObjectHolder<AnyObject>()

            let provider = subscribeChatsSnapshot(
                for: predicate,
                deliverOn: syncQueue,
                update: { models in
                    continuation.yield(models)
                },
                failure: { error in
                    continuation.yield(with: .failure(error))
                }
            )

            holder.set(provider)

            continuation.onTermination = { @Sendable _ in
                holder.set(nil)
            }
        }
        .eraseToAnyAsyncSequence()
    }
}
