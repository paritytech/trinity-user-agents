import CoreData
import Foundation
import Operation_iOS
import Testing

@testable import polkadot_app

extension CoreDataMapperTests {
    @Suite("PushNotificationChatMessageEntityMapper")
    struct PushNotificationChatMessageEntityMapperTests {
        private let facade = UserDataStorageTestFacade()

        @Test("transform is unsupported")
        func transformThrowsUnsupported() async throws {
            let mapper = PushNotificationChatMessageEntityMapper()

            try await withContext { context in
                let entity = CDChatMessage(context: context)

                #expect(throws: CoreDataMapperError.self) {
                    _ = try mapper.transform(entity: entity)
                }
            }
        }

        @Test("populate delegates to base mapper when messageId is nil")
        func populateWhenNoMessageIdSet() async throws {
            let mapper = PushNotificationChatMessageEntityMapper()
            let chatId = Chat.Id.person(Data(repeating: 0x01, count: 32))
            let model = Chat.LocalMessage(
                messageId: "new-id",
                chatId: chatId,
                origin: .contact(Data(repeating: 0x02, count: 32)),
                creationSource: .localDevice,
                status: .incoming(.seen),
                timestamp: 42,
                content: .text("hello"),
                reactions: [],
                compactionId: nil,
                relatedMessages: []
            )

            try await withContext { context in
                let chatEntity = CDChat(context: context)
                chatEntity.identifier = chatId.rawRepresentation
                chatEntity.chatType = Int16(chatId.chatType)
                chatEntity.chatTypeContext = chatId.chatTypeContext

                let entity = CDChatMessage(context: context)

                try mapper.populate(entity: entity, from: model, using: context)

                #expect(entity.messageId == model.messageId)
                #expect(entity.timestamp == Int64(bitPattern: model.timestamp))
                #expect(entity.status == model.status.rawValue)
                #expect(entity.originType == model.origin.rawType)
                #expect(entity.originKey == model.origin.rawKey)
                #expect(entity.contentType == Int16(model.content.contentType.rawValue))
                #expect(entity.content != nil)
                #expect(entity.chat === chatEntity)
                #expect(chatEntity.lastDisplayMessage === entity)
            }
        }

        @Test("populate is a no-op when messageId is already set")
        func populateSkipsWhenMessageIdSet() async throws {
            let mapper = PushNotificationChatMessageEntityMapper()
            let model = Chat.LocalMessage(
                messageId: "abc",
                chatId: .person(Data(repeating: 0x01, count: 32)),
                origin: .contact(Data(repeating: 0x02, count: 32)),
                creationSource: .localDevice,
                status: .incoming(.new),
                timestamp: 42,
                content: .text("hello"),
                reactions: [],
                compactionId: nil,
                relatedMessages: []
            )

            try await withContext { context in
                let entity = CDChatMessage(context: context)
                entity.messageId = "existing"
                entity.status = Chat.LocalMessage.Status.incoming(.seen).rawValue
                entity.timestamp = 7

                try mapper.populate(entity: entity, from: model, using: context)

                #expect(entity.messageId == "existing")
                #expect(entity.status == Chat.LocalMessage.Status.incoming(.seen).rawValue)
                #expect(entity.timestamp == 7)
                #expect(entity.chat == nil)
                #expect(entity.content == nil)
            }
        }
    }
}

private extension CoreDataMapperTests.PushNotificationChatMessageEntityMapperTests {
    func withContext(_ block: @escaping (NSManagedObjectContext) throws -> Void) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            facade.databaseService.performAsync { context, error in
                guard let context else {
                    continuation.resume(throwing: error ?? CoreDataRepositoryError.undefined)
                    return
                }

                context.performAndWait {
                    do {
                        try block(context)
                        continuation.resume(returning: ())
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }
        }
    }
}
