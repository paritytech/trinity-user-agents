import Foundation
import CoreData
import Operation_iOS

final class ChatMessageReactionMapper {
    typealias DataProviderModel = Chat.MessageReaction
    typealias CoreDataEntity = CDChatMessageReaction

    init() {}
}

extension ChatMessageReactionMapper: CoreDataMapperProtocol {
    var entityIdentifierFieldName: String {
        #keyPath(CDChatMessageReaction.identifier)
    }

    enum MapperError: Error {
        case missingRequiredData(String)
        case invalidOrigin
        case invalidChatId
        case missingMessage
    }

    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let messageId = entity.message?.messageId else {
            throw MapperError.missingRequiredData("messageId")
        }

        guard let emoji = entity.emoji else {
            throw MapperError.missingRequiredData("emoji")
        }

        guard
            let origin = Chat.LocalMessage.Origin(
                rawType: entity.originType,
                rawKey: entity.originKey
            ) else {
            throw MapperError.invalidOrigin
        }

        guard
            let rawChatId = entity.message?.chat?.identifier,
            let chatId = Chat.Id.fromRawRepresentation(rawChatId) else {
            throw MapperError.invalidChatId
        }

        let timestamp = UInt64(bitPattern: entity.timestamp)

        return Chat.MessageReaction(
            messageId: messageId,
            emoji: emoji,
            origin: origin,
            chatId: chatId,
            timestamp: timestamp
        )
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.emoji = model.emoji
        entity.originType = model.origin.rawType
        entity.originKey = model.origin.rawKey
        entity.timestamp = Int64(bitPattern: model.timestamp)

        let messageEntity: CDChatMessage = try context
            .first(for: .chatMessage(with: model.messageId))
            .mapOrThrow(MapperError.missingMessage)

        entity.message = messageEntity
    }
}
