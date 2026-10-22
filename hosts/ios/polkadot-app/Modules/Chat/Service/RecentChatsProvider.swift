import Foundation
import SubstrateSdk
import AsyncExtensions

final class RecentChatsProvider {
    private let chatProvider: ChatContactDataProviderMaking
    /// The chat product runs the person chats: its contacts are the recent chats (HX2).
    private let productChat: Bool

    init(
        chatProvider: ChatContactDataProviderMaking,
        productChat: Bool = false
    ) {
        self.chatProvider = chatProvider
        self.productChat = productChat
    }

    func subscribe() -> AnyAsyncSequence<[SearchRow<ContactSearchPayload>]> {
        guard !productChat else {
            return subscribeProductContacts()
        }

        return chatProvider.subscribeChatsWithPredicate(NSPredicate.chatWithNonBlockedContact())
            .map { chats in
                chats.compactMap { chat -> SearchRow<ContactSearchPayload>? in
                    guard case let .person(contact) = chat.peer else {
                        return nil
                    }

                    guard !contact.hasIncomingChatRequest else {
                        return nil
                    }

                    return SearchRow(
                        accountId: contact.accountId,
                        username: Username(value: contact.username),
                        matchTerms: [contact.username],
                        payload: .local(contact)
                    )
                }
            }
            .eraseToAnyAsyncSequence()
    }
}

private extension RecentChatsProvider {
    /// The contacts of the chat product that the chat list shows, without the requests that wait for the user.
    func subscribeProductContacts() -> AnyAsyncSequence<[SearchRow<ContactSearchPayload>]> {
        chatProvider.subscribeChatsWithPredicate(.listedChats(productChat: true))
            .map { chats in
                chats.compactMap { chat -> SearchRow<ContactSearchPayload>? in
                    guard
                        let product = ProductContact(chat: chat),
                        product.state != .incoming,
                        !product.isBlocked
                    else {
                        return nil
                    }

                    return SearchRow(
                        accountId: product.accountId,
                        username: Username(value: product.username),
                        matchTerms: [product.username],
                        payload: .local(product.contact)
                    )
                }
            }
            .eraseToAnyAsyncSequence()
    }
}
