import Foundation
import AsyncExtensions
import Foundation_iOS
import StructuredConcurrency

extension ChatContactDataProviderMaking {
    func subscribeAllContacts() -> AnyAsyncSequence<[Chat.Contact]> {
        subscribeContactsWithPredicate(nil)
    }

    func subscribeBlockedContacts() -> AnyAsyncSequence<[Chat.Contact]> {
        subscribeContactsWithPredicate(.blockedContacts())
    }

    func subscribeContactsWithPredicate(
        _ predicate: NSPredicate?
    ) -> AnyAsyncSequence<[Chat.Contact]> {
        let syncQueue = DispatchQueue(label: "io.chat.contacts.provider.async.updates")

        return AsyncThrowingStream { continuation in
            let holder = AnyObjectHolder<AnyObject>()

            let provider = subscribeContactsSnapshot(
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

    func subscribeActiveIcomingChatRequests() -> AnyAsyncSequence<[Chat.LocalModel]> {
        subscribeChatsWithPredicate(
            .chatWithActiveIncomingRequests()
        )
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
