import CoreData
import Foundation
import Testing
@testable import polkadot_app

struct ChatPendingMessagesServiceTests {
    private let facade = UserDataStorageTestFacade()
    private let peer = Data(repeating: 0x0A, count: 32)
    private let otherPeer = Data(repeating: 0x0B, count: 32)

    @Test func failsOnlyNewLocalMessagesToTheContact() async throws {
        try await insert("new", to: peer, status: .outgoing(.new))
        try await insert("sent", to: peer, status: .outgoing(.sent))
        try await insert("synced", to: peer, status: .outgoing(.new), source: .deviceSync)
        try await insert("other", to: otherPeer, status: .outgoing(.new))

        try await makeService().failPendingMessages(to: peer)

        #expect(try await status(of: "new") == .outgoing(.failed))
        #expect(try await status(of: "sent") == .outgoing(.sent))
        #expect(try await status(of: "synced") == .outgoing(.new))
        #expect(try await status(of: "other") == .outgoing(.new))
        #expect(try await !makeService().hasPendingMessages(to: peer))
    }

    @Test func keepsTheAcceptanceRenewable() async throws {
        try await insert("acceptance", to: peer, status: .outgoing(.new), content: acceptanceContent())

        let service = makeService()
        try await service.failPendingMessages(to: peer)

        #expect(try await status(of: "acceptance") == .outgoing(.new))
        #expect(try await !service.hasPendingMessages(to: peer))
    }
}

private extension ChatPendingMessagesServiceTests {
    func makeService() -> ChatPendingMessagesService {
        ChatPendingMessagesService(repositoryFactory: ChatMessageStatusUpdateRepositoryFactory(storageFacade: facade))
    }

    func acceptanceContent() -> Chat.LocalMessage.Content {
        .multiChatAccepted(.init(
            requestId: "request",
            device: Chat.PeerDevice(
                statementAccountId: Data(repeating: 2, count: 32),
                encryptionPublicKey: Data(repeating: 3, count: 32)
            )
        ))
    }

    func insert(
        _ messageId: String,
        to contactId: Data,
        status: Chat.LocalMessage.Status,
        source: Chat.LocalMessage.CreationSource = .localDevice,
        content: Chat.LocalMessage.Content = .text("hello")
    ) async throws {
        let message = Chat.LocalMessage(
            messageId: messageId,
            chatId: .person(contactId),
            origin: .user,
            creationSource: source,
            status: status,
            timestamp: Date().toChatTimestamp(),
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        try await facade.databaseService.performWrite { context in
            try insertChatIfNeeded(for: contactId, in: context)
            try ChatMessageEntityMapper(orderAllocator: InMemoryChatMessageOrderAllocator()).populate(
                entity: CDChatMessage(context: context),
                from: message,
                using: context
            )
        }
    }

    func insertChatIfNeeded(for contactId: Data, in context: NSManagedObjectContext) throws {
        let request = CDChat.fetchRequest()
        request.predicate = NSPredicate(
            format: "%K == %@",
            #keyPath(CDChat.identifier),
            Chat.Id.person(contactId).rawRepresentation
        )

        guard try context.fetch(request).isEmpty else { return }

        let contact = CDChatContact(context: context)
        contact.identifier = contactId.toHex()
        contact.username = "peer"
        contact.publicKey = Data(repeating: 1, count: 32)
        contact.ownSignKeyId = "//wallet//main"

        let chat = CDChat(context: context)
        chat.identifier = Chat.Id.person(contactId).rawRepresentation
        chat.chatType = Int16(Chat.Id.ChatType.person.rawValue)
        chat.contact = contact
    }

    func status(of messageId: String) async throws -> Chat.LocalMessage.Status? {
        try await ChatMessageStatusUpdateRepositoryFactory(storageFacade: facade)
            .createRepository(forFilter: nil)
            .fetchOperation(by: { messageId }, options: .init())
            .asyncExecute()?
            .status
    }
}
