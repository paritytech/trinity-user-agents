import CoreData
import Foundation
import Operation_iOS

extension Chat {
    /// Turns a stored message into the deleted tombstone.
    struct MessageDeletion: Identifiable {
        let messageId: MessageId

        var identifier: String {
            messageId
        }
    }
}

/// Writes the deleted tombstone into an existing message row and leaves the row in place, so replies and the
/// extension history keep a valid reference. The mapper also removes the reactions of the message, and the
/// unread marks of the message, of its edits and of the reaction events on it. When the message was the last
/// message of its chat, the newest message that the chat list can show takes its place.
final class ChatMessageDeletionMapper {
    typealias DataProviderModel = Chat.MessageDeletion
    typealias CoreDataEntity = CDChatMessage

    var entityIdentifierFieldName: String {
        #keyPath(CDChatMessage.messageId)
    }
}

extension ChatMessageDeletionMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case missingMessage
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.messageId != nil, let chat = entity.chat else {
            throw MappingError.missingMessage
        }

        let tombstone = Chat.LocalMessage.Content.deleted
        entity.contentType = Int16(tombstone.contentType.rawValue)
        entity.contentKey = nil
        entity.content?.data = try tombstone.scaleEncoded()
        entity.searchableText = ChatSearchText.stored(for: tombstone)
        entity.markModified()

        for reaction in (entity.reactions as? Set<CDChatMessageReaction>) ?? [] {
            context.delete(reaction)
        }

        try removeUnreadMarks(relatedTo: model.messageId, in: chat, using: context)

        if chat.lastDisplayMessage == entity {
            chat.lastDisplayMessage = try newestDisplayMessage(in: chat, excluding: entity, using: context)
        }
    }
}

private extension ChatMessageDeletionMapper {
    static let excludedContentTypes: [NSNumber] = (UInt8.min ... UInt8.max)
        .compactMap(Chat.LocalMessage.Content.ContentType.init(rawValue:))
        .filter(\.isExcludedFromChatList)
        .map { NSNumber(value: Int16($0.rawValue)) }

    func removeUnreadMarks(
        relatedTo messageId: Chat.MessageId,
        in chat: CDChat,
        using context: NSManagedObjectContext
    ) throws {
        let marks = (chat.unreadMessages as? Set<CDChatUnreadMessage>) ?? []
        let markedIds = marks.compactMap(\.messageId)

        guard !markedIds.isEmpty else {
            return
        }

        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = .messages(withIds: markedIds)
        let markedMessages = try context.fetch(request).reduce(into: [String: CDChatMessage]()) { result, row in
            row.messageId.map { result[$0] = row }
        }

        for mark in marks {
            guard let markedId = mark.messageId, let row = markedMessages[markedId] else {
                continue
            }

            if markedId == messageId || row.groupingId == messageId || reactionTarget(of: row) == messageId {
                context.delete(mark)
            }
        }
    }

    func reactionTarget(of row: CDChatMessage) -> Chat.MessageId? {
        guard row.isReaction else {
            return nil
        }

        switch try? ChatMessageEntityMapper.getContent(from: row) {
        case let .reacted(content),
             let .reactionRemoved(content):
            return content.messageId
        default:
            return nil
        }
    }

    func newestDisplayMessage(
        in chat: CDChat,
        excluding deleted: CDChatMessage,
        using context: NSManagedObjectContext
    ) throws -> CDChatMessage? {
        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            .messages(in: chat),
            NSPredicate(format: "SELF != %@", deleted),
            NSPredicate(format: "NOT (%K IN %@)", #keyPath(CDChatMessage.contentType), Self.excludedContentTypes)
        ])
        request.sortDescriptors = [
            NSSortDescriptor(key: #keyPath(CDChatMessage.order), ascending: false),
            NSSortDescriptor(key: #keyPath(CDChatMessage.timestamp), ascending: false)
        ]
        request.fetchLimit = 1

        return try context.fetch(request).first
    }
}
