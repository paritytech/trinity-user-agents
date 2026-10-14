import Foundation
import Operation_iOS

protocol ChatOpenModelResolving {
    func resolveOpenModel(for contact: Chat.RemoteContact) async throws -> ChatOpenModel
}

final class ChatOpenModelResolver: ChatOpenModelResolving {
    private let chatRepositoryFactory: ChatRepositoryMaking

    init(chatRepositoryFactory: ChatRepositoryMaking = ChatRepositoryFactory()) {
        self.chatRepositoryFactory = chatRepositoryFactory
    }

    func resolveOpenModel(for contact: Chat.RemoteContact) async throws -> ChatOpenModel {
        let chatRepository = chatRepositoryFactory.createRepository(
            forFilter: .contact(for: contact.accountId)
        )

        let chats = try await chatRepository.fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()

        if let chat = chats.first {
            return .existingChat(chat.chatId)
        }

        let newRequest = try ChatOpenModel.NewRequest(
            remoteContact: contact,
            ownKeyId: Chat.Contact.Own.main()
        )

        return .newRequest(newRequest)
    }
}
