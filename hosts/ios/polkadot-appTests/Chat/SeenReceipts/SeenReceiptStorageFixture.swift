import CoreData
import Foundation
import Operation_iOS

@testable import polkadot_app

struct SeenReceiptStorageFixture {
    let facade = UserDataStorageTestFacade()
    let peerAccountId = Data(repeating: 0x01, count: 32)

    var chatId: Chat.Id {
        .person(peerAccountId)
    }

    func seedChat(isBlocked: Bool = false, pendingRequest: Chat.Request? = nil) async throws {
        let peer = makePeer(isBlocked: isBlocked)

        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatContactMapper()))
            .saveOperation({ [peer] }, { [] })
            .asyncExecute()

        if let pendingRequest {
            try await attachPendingRequest(pendingRequest)
        }

        try await facade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
            .saveOperation({ [.newChatWithContact(peer)] }, { [] })
            .asyncExecute()
    }

    func makePeer(isBlocked: Bool = false) -> Chat.Contact {
        Chat.Contact(
            accountId: peerAccountId,
            username: "Alice",
            publicKey: Data(repeating: 0x02, count: 32),
            pin: nil,
            pushId: nil,
            pushToken: nil,
            voipPushToken: nil,
            peerPlatform: nil,
            lastOwnToken: nil,
            voipLastOwnToken: nil,
            chatRequest: nil,
            ownKeyId: .init(signKeyId: "sign-key", encryptionKeyId: "encryption-key"),
            imageData: nil,
            isBlocked: isBlocked,
            devices: [],
            pendingDevicesFanOut: false
        )
    }

    func insertMessage(_ message: Chat.LocalMessage, order: Int64) async throws {
        try await facade.databaseService.performWrite { context in
            let entity = try context.insertNew(CDChatMessage.self)
            try ChatMessageEntityMapper().populate(
                entity: entity,
                from: message,
                inheritingOrder: order,
                using: context
            )
        }
    }

    func makeMessage(
        id: Chat.MessageId,
        status: Chat.LocalMessage.Status,
        timestamp: Chat.Timestamp,
        content: Chat.LocalMessage.Content = .text("text")
    ) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: id,
            chatId: chatId,
            origin: status.isOutgoing ? .user : .contact(peerAccountId),
            creationSource: .localDevice,
            status: status,
            timestamp: timestamp,
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }

    func peerSeenOrder() async throws -> Int64? {
        try await facade.databaseService.performRead { [chatId] context in
            let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))
            return chat?.peerSeenOrder
        }
    }

    func lastSentSeenMark() async throws -> (order: Int64, timestamp: Int64) {
        try await facade.databaseService.performRead { [chatId] context in
            let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))
            return (chat?.lastSentSeenOrder ?? 0, chat?.lastSentSeenTimestamp ?? 0)
        }
    }

    func seedLastSentSeen(order: Int64, timestamp: Int64) async throws {
        try await facade.databaseService.performWrite { [chatId] context in
            let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))
            chat?.lastSentSeenOrder = order
            chat?.lastSentSeenTimestamp = timestamp
        }
    }

    func hasPendingRequest() async throws -> Bool {
        try await facade.databaseService.performRead { [peerAccountId] context in
            let contact: CDChatContact? = try context.first(for: .contact(for: peerAccountId))
            return contact?.chatRequest != nil
        }
    }

    func messageCount() async throws -> Int {
        try await facade.databaseService.performRead { context in
            try context.count(for: CDChatMessage.fetchRequest())
        }
    }
}

private extension SeenReceiptStorageFixture {
    func attachPendingRequest(_ request: Chat.Request) async throws {
        try await facade.databaseService.performWrite { [peerAccountId] context in
            let contact: CDChatContact? = try context.first(for: .contact(for: peerAccountId))
            let requestEntity = try context.insertNew(CDChatRequest.self)
            contact?.chatRequest = requestEntity
            try ChatRequestMapper().populate(entity: requestEntity, from: request, using: context)
        }
    }
}
