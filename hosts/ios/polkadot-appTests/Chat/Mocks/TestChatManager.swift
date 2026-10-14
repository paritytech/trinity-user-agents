import Foundation
import Operation_iOS

@testable import polkadot_app

final class TestChatManager {
    let peer: Chat.Peer
    private let contactRepository: AnyDataProviderRepository<Chat.Contact>
    private let chatRepository: AnyDataProviderRepository<Chat.LocalModel>
    private let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>

    var chatId: Chat.Id { peer.chatId }

    init(peer: Chat.Peer, facade: StorageFacadeProtocol) {
        self.peer = peer

        contactRepository = ChatContactRepositoryFactory(storageFacade: facade)
            .createRepository(forFilter: nil)

        chatRepository = ChatRepositoryFactory(storageFacade: facade)
            .createRepository(forFilter: nil)

        messageRepository = ChatMessageRepositoryFactory(storageFacade: facade)
            .createRepository(forFilter: nil)
    }

    func setup() async throws {
        let chatExists = try await chatRepository.fetchOperation(
            by: { [chatId] in chatId.rawRepresentation },
            options: RepositoryFetchOptions()
        ).asyncExecute() != nil

        guard !chatExists else { return }

        if let contact = peer.contact {
            try await contactRepository.saveOperation({ [contact] }, { [] }).asyncExecute()
        }

        let chat = Chat.LocalModel(
            peer: peer,
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: Date(),
            roomMetadata: nil
        )

        try await chatRepository.saveOperation({ [chat] }, { [] }).asyncExecute()
    }

    func sendMessage(
        _ content: Chat.LocalMessage.Content,
        status: Chat.LocalMessage.Status
    ) async throws -> Chat.LocalMessage {
        let origin: Chat.LocalMessage.Origin =
            switch status {
            case .incoming:
                if let accountId = chatId.accountId {
                    .contact(accountId)
                } else {
                    .user
                }
            case .outgoing:
                .user
            }

        let message = Chat.LocalMessage(
            messageId: UUID().uuidString,
            chatId: chatId,
            origin: origin,
            creationSource: .localDevice,
            status: status,
            timestamp: UInt64(Date().timeIntervalSince1970),
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        try await messageRepository.saveOperation({ [message] }, { [] }).asyncExecute()

        return message
    }
}
