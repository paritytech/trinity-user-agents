import Foundation
import Operation_iOS
import CoreData
import SubstrateSdk
import Foundation_iOS

final class ChatModelMapper {
    typealias DataProviderModel = Chat.LocalModel
    typealias CoreDataEntity = CDChat

    var entityIdentifierFieldName: String {
        #keyPath(CDChat.identifier)
    }
}

private extension ChatModelMapper {
    func ensureNoContactSet(_ entity: CDChat) -> Bool {
        entity.contact == nil
    }
}

extension ChatModelMapper: CoreDataMapperProtocol {
    enum MapperError: Error {
        case invalidParticipant
        case missingContact
        case missingChatRequest
        case chatContactExists
        case chatRequestExists
    }

    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard
            let rawId = entity.identifier,
            let chatId = Chat.Id.fromRawRepresentation(rawId) else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDChat.identifier)
            )
        }

        let lastDisplayMessage = try entity.lastDisplayMessage.map {
            try ChatMessageEntityMapper().transform(entity: $0)
        }

        let hasIncomingReaction = Self.hasUnreadReaction(for: entity)

        let roomMetadata = try (entity.roomMetadata).map {
            try ChatRoomMetadataEntityMapper().transform(entity: $0)
        }

        switch chatId {
        case .person:
            guard let contactEntity = entity.contact else {
                throw MapperError.missingContact
            }

            let contact = try ChatContactMapper().transform(entity: contactEntity)

            let unreadCount = Self.nonReactionUnreadCount(for: entity)

            return DataProviderModel(
                peer: .person(contact),
                message: lastDisplayMessage,
                unreadDisplayMessageCount: unreadCount,
                hasIncomingReaction: hasIncomingReaction,
                createdAt: entity.createdAt,
                roomMetadata: roomMetadata
            )
        case let .chatExtension(extId, roomId):
            let unreadCount = Self.nonReactionUnreadCount(for: entity)

            return DataProviderModel(
                peer: .chatExtension(extId, roomId: roomId),
                message: lastDisplayMessage,
                unreadDisplayMessageCount: unreadCount,
                hasIncomingReaction: hasIncomingReaction,
                createdAt: entity.createdAt,
                roomMetadata: roomMetadata,
                roomFooter: entity.footer.flatMap(Chat.RoomFooter.init(rawValue:))
            )
        }
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.chatType = Int16(model.chatId.chatType)
        entity.chatTypeContext = model.chatId.chatTypeContext

        guard ensureNoContactSet(entity) else {
            return
        }

        switch model.peer {
        case let .person(contact):
            let contactEntity: CDChatContact = try context
                .first(for: .contact(for: contact.accountId))
                .mapOrThrow(MapperError.missingContact)

            entity.contact = contactEntity
        case .chatExtension:
            entity.createdAt = model.createdAt

            if let metadata = model.roomMetadata {
                let metadataEntity = try entity.roomMetadata ?? context.insertNew(CDChatRoomMetadata.self)

                ChatRoomMetadataEntityMapper().populate(entity: metadataEntity, from: metadata)

                entity.roomMetadata = metadataEntity
            } else if let existing = entity.roomMetadata {
                context.delete(existing)
                entity.roomMetadata = nil
            }
        }
    }
}

private extension ChatModelMapper {
    static let reactionContentTypes: [Int16] = [
        Int16(Chat.LocalMessage.Content.ContentType.reacted.rawValue),
        Int16(Chat.LocalMessage.Content.ContentType.reactionRemoved.rawValue)
    ]

    static func nonReactionUnreadCount(for chat: CDChat) -> Int {
        let unreadIds = (chat.unreadMessages as? Set<CDChatUnreadMessage>)?.compactMap(\.messageId) ?? []
        guard !unreadIds.isEmpty else { return 0 }

        let messages = (chat.messages as? Set<CDChatMessage>) ?? []
        let unreadIdSet = Set(unreadIds)

        let unreadGroupingIds: Set<String> = Set(
            messages.compactMap { message -> String? in
                guard
                    let messageId = message.messageId,
                    unreadIdSet.contains(messageId),
                    !reactionContentTypes.contains(message.contentType)
                else { return nil }
                return message.groupingId ?? messageId
            }
        )

        return unreadGroupingIds.count
    }

    static func hasUnreadReaction(for chat: CDChat) -> Bool {
        let unreadIds = (chat.unreadMessages as? Set<CDChatUnreadMessage>)?.compactMap(\.messageId) ?? []
        guard !unreadIds.isEmpty else { return false }

        let messages = (chat.messages as? Set<CDChatMessage>) ?? []
        let unreadIdSet = Set(unreadIds)

        return messages.contains {
            guard let messageId = $0.messageId else { return false }
            return unreadIdSet.contains(messageId) && reactionContentTypes.contains($0.contentType)
        }
    }
}

extension Chat.LocalModel: Identifiable {
    var identifier: String {
        chatId.rawRepresentation
    }
}
