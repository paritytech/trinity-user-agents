import Foundation
import CoreData
import Operation_iOS
import SubstrateSdk

final class CompactionCommitMapper {
    struct Model: Identifiable {
        let compactedRemoteMessage: Chat.RemoteMessage
        let originalMessageIds: [String]

        var identifier: String { compactedRemoteMessage.messageId }
    }

    typealias DataProviderModel = Model
    typealias CoreDataEntity = CDChatMessage

    var entityIdentifierFieldName: String {
        #keyPath(CDChatMessage.messageId)
    }
}

extension CompactionCommitMapper: CoreDataMapperProtocol {
    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard let firstOriginalId = model.originalMessageIds.first else {
            throw CoreDataMapperError.unexpected("No original messages")
        }

        let originalEntity: CDChatMessage = try context
            .first(for: .chatMessage(with: firstOriginalId))
            .mapOrThrow(CoreDataMapperError.unexpected("Original message not found"))

        guard
            let rawChatId = originalEntity.chat?.identifier,
            let chatId = Chat.Id.fromRawRepresentation(rawChatId)
        else {
            throw CoreDataMapperError.unexpected("Cannot derive chatId from original message")
        }

        let compactedMessage = try createCompactedLocalMessage(
            from: model,
            chatId: chatId
        )

        let fullMessageMapper = ChatMessageEntityMapper()
        try fullMessageMapper.populate(entity: entity, from: compactedMessage, using: context)

        let compactedMessageId = model.compactedRemoteMessage.messageId

        for originalMessageId in model.originalMessageIds {
            let originalMsg: CDChatMessage? = try context.first(
                for: .chatMessage(with: originalMessageId)
            )

            originalMsg?.compactionId = compactedMessageId
        }
    }
}

private extension CompactionCommitMapper {
    func createCompactedLocalMessage(
        from model: Model,
        chatId: Chat.Id
    ) throws -> Chat.LocalMessage {
        guard
            let message = Chat.LocalMessage(
                remote: model.compactedRemoteMessage,
                creationSource: .localDevice,
                status: .outgoing(.new),
                chatId: chatId,
                origin: .user
            ),
            message.content.contentType == .compactedMessages
        else {
            throw CoreDataMapperError.unexpected("Not a compacted message")
        }

        return message
    }
}
