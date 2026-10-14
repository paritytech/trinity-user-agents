import Foundation
import SubstrateSdk
import AsyncExtensions

final class RecentChatsProvider {
    private let chatProvider: ChatContactDataProviderMaking

    init(
        chatProvider: ChatContactDataProviderMaking
    ) {
        self.chatProvider = chatProvider
    }

    func subscribe() -> AnyAsyncSequence<[SearchRow<ContactSearchPayload>]> {
        chatProvider.subscribeChatsWithPredicate(NSPredicate.chatWithNonBlockedContact())
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
