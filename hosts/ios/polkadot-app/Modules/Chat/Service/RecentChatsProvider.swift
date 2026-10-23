import Foundation
import SubstrateSdk
import AsyncExtensions

/// The contacts of the chat product that the chat list shows, without the requests that wait for the user (HX2).
final class RecentChatsProvider {
    private let chatProvider: ChatContactDataProviderMaking

    init(chatProvider: ChatContactDataProviderMaking) {
        self.chatProvider = chatProvider
    }

    func subscribe() -> AnyAsyncSequence<[SearchRow<ContactSearchPayload>]> {
        chatProvider.subscribeChatsWithPredicate(.listedChats())
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
