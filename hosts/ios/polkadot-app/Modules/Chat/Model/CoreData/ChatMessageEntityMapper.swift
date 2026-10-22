import Foundation
import CoreData
import SubstrateSdk
import SubstrateSdkExt
import Operation_iOS
import BigInt
import Foundation_iOS

final class ChatMessageEntityMapper {
    typealias DataProviderModel = Chat.LocalMessage
    typealias CoreDataEntity = CDChatMessage

    private let orderAllocator: ChatMessageOrderAllocating

    var entityIdentifierFieldName: String {
        #keyPath(CDChatMessage.messageId)
    }

    init(orderAllocator: ChatMessageOrderAllocating = FileChatMessageOrderAllocator.shared) {
        self.orderAllocator = orderAllocator
    }
}

extension ChatMessageEntityMapper: CoreDataMapperProtocol {
    enum MapperError: Error {
        case invalidStatus(Int16)
        case missingChat
        case invalidMessage
        case invalidChatId
        case invalidOrigin
    }

    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let messageId = entity.messageId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDChatMessage.messageId)
            )
        }

        guard let chat = entity.chat else {
            throw MapperError.missingChat
        }

        guard
            let rawId = chat.identifier,
            let chatId = Chat.Id.fromRawRepresentation(rawId) else {
            throw MapperError.invalidChatId
        }

        guard
            let origin = Chat.LocalMessage.Origin(
                rawType: entity.originType,
                rawKey: entity.originKey
            ) else {
            throw MapperError.invalidOrigin
        }

        let statusRawValue = entity.status
        guard let status = Chat.LocalMessage.Status(rawValue: statusRawValue) else {
            throw MapperError.invalidStatus(statusRawValue)
        }

        let timestamp = UInt64(bitPattern: entity.timestamp)

        let content = try Self.getContent(from: entity).attachingTransferState(of: entity)

        let reactions = getReactions(from: entity, chatId: chatId)

        let relatedMessages = getRelatedMessages(from: entity)
        let creationSource = Chat.LocalMessage.CreationSource(
            rawValue: entity.creationSource
        ) ?? .localDevice

        let order = UInt64(bitPattern: entity.order)

        return Chat.LocalMessage(
            messageId: messageId,
            chatId: chatId,
            origin: origin,
            creationSource: creationSource,
            status: status,
            timestamp: timestamp,
            content: content,
            reactions: reactions,
            compactionId: entity.compactionId,
            relatedMessages: relatedMessages,
            order: order
        )
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        try populate(entity: entity, from: model, inheritingOrder: nil, using: context)
    }
}

extension ChatMessageEntityMapper {
    /// `inheritedOrder` places a new row at an existing position, e.g. messages expanded from a compacted one.
    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        inheritingOrder inheritedOrder: Int64?,
        using context: NSManagedObjectContext
    ) throws {
        guard let messageState = ensureValidMessage(entity: entity, from: model) else {
            throw MapperError.invalidMessage
        }

        let shouldMarkModified = hasMessageDataChanges(entity: entity, model: model)

        entity.messageId = model.messageId
        entity.timestamp = Int64(bitPattern: model.timestamp)
        entity.contentType = Int16(model.content.contentType.rawValue)
        entity.contentKey = model.content.contentKey
        entity.originType = model.origin.rawType
        entity.originKey = model.origin.rawKey
        entity.groupingId = model.groupingId

        if messageState.isNew {
            entity.creationSource = model.creationSource.rawValue
        }

        try populateContent(model.content, to: entity, using: context)

        if shouldMarkModified {
            entity.markModified()
        }

        try groupRelatedMessages(entity: entity, groupingId: model.groupingId, context: context)

        let chat: CDChat = try context
            .first(for: .chat(for: model.chatId.rawRepresentation))
            .mapOrThrow(MapperError.missingChat)

        if messageState.isNew {
            entity.order = try inheritedOrder ?? newOrder(for: model, in: chat, using: context)
        }

        entity.chat = chat

        try updateChatBasedOnNewMessage(
            chatEntity: chat,
            message: model,
            messageState: messageState,
            messageEntity: entity,
            context: context
        )

        entity.touchParent()
    }
}

extension ChatMessageEntityMapper {
    static func getContent(from entity: CoreDataEntity) throws -> Chat.LocalMessage.Content {
        guard let encodedContent = entity.content?.data else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDChatMessage.content)
            )
        }

        return try Chat.LocalMessage.Content.fromScaleEncoded(encodedContent)
    }
}

private extension ChatMessageEntityMapper {
    func getReactions(from entity: CoreDataEntity, chatId: Chat.Id) -> [Chat.MessageReaction] {
        guard let reactionEntities = entity.reactions as? Set<CDChatMessageReaction> else {
            return []
        }

        return reactionEntities.compactMap { reactionEntity -> Chat.MessageReaction? in
            guard
                let messageId = entity.messageId,
                let emoji = reactionEntity.emoji,
                let origin = Chat.LocalMessage.Origin(
                    rawType: reactionEntity.originType,
                    rawKey: reactionEntity.originKey
                )
            else {
                return nil
            }

            let timestamp = UInt64(bitPattern: reactionEntity.timestamp)

            return Chat.MessageReaction(
                messageId: messageId,
                emoji: emoji,
                origin: origin,
                chatId: chatId,
                timestamp: timestamp
            )
        }
    }

    func populateContent(
        _ content: DataProviderModel.Content,
        to entity: CoreDataEntity,
        using context: NSManagedObjectContext
    ) throws {
        if entity.content == nil {
            entity.content = try context.insertNew(CDMessageContent.self)
        }

        entity.content?.data = try content.scaleEncoded()
    }

    func hasMessageDataChanges(entity: CoreDataEntity, model: DataProviderModel) -> Bool {
        guard entity.messageId != nil else {
            return true
        }

        if entity.timestamp != Int64(bitPattern: model.timestamp) {
            return true
        }

        if entity.contentType != Int16(model.content.contentType.rawValue) {
            return true
        }

        if entity.contentKey != model.content.contentKey {
            return true
        }

        if entity.originType != model.origin.rawType || entity.originKey != model.origin.rawKey {
            return true
        }

        if entity.groupingId != model.groupingId {
            return true
        }

        if entity.chat?.identifier != model.chatId.rawRepresentation {
            return true
        }

        let newContentData = try? model.content.scaleEncoded()
        return entity.content?.data != newContentData
    }

    func newOrder(
        for model: DataProviderModel,
        in chat: CDChat,
        using context: NSManagedObjectContext
    ) throws -> Int64 {
        if model.creationSource == .deviceSync,
           let latestTimestamp = try latestTimestamp(in: chat, using: context),
           model.timestamp < latestTimestamp {
            return try backlogOrder(for: model, in: chat, using: context)
        }

        let order = try orderAllocator.nextOrder { try highestStoredOrder(using: context) }
        return Int64(bitPattern: order)
    }

    /// Served by `byChatTimestampIndex (chat, timestamp)`: a single seek instead of scanning the chat.
    /// Runs for every new device-sync message, so a bulk backlog import would otherwise cost O(k·n).
    func latestTimestamp(
        in chat: CDChat,
        using context: NSManagedObjectContext
    ) throws -> UInt64? {
        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = .messages(in: chat)
        request.sortDescriptors = [NSSortDescriptor(key: #keyPath(CDChatMessage.timestamp), ascending: false)]
        request.fetchLimit = 1

        return try context.fetch(request).first.map { UInt64(bitPattern: $0.timestamp) }
    }

    /// Floor for an empty or unreadable counter so new rows never sort above history.
    func highestStoredOrder(using context: NSManagedObjectContext) throws -> UInt64 {
        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.sortDescriptors = [NSSortDescriptor(key: #keyPath(CDChatMessage.order), ascending: false)]
        request.fetchLimit = 1

        return try context.fetch(request).first.map { UInt64(bitPattern: $0.order) } ?? 0
    }

    /// Backlog rows borrow the order of the newest preceding message so they interleave by timestamp.
    func backlogOrder(
        for model: DataProviderModel,
        in chat: CDChat,
        using context: NSManagedObjectContext
    ) throws -> Int64 {
        let notAfter = NSPredicate(
            format: "%K <= %lld",
            #keyPath(CDChatMessage.timestamp),
            Int64(bitPattern: model.timestamp)
        )

        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            .messages(in: chat),
            notAfter
        ])
        request.sortDescriptors = [
            NSSortDescriptor(key: #keyPath(CDChatMessage.order), ascending: false),
            NSSortDescriptor(key: #keyPath(CDChatMessage.timestamp), ascending: false)
        ]
        request.fetchLimit = 1

        return try context.fetch(request).first?.order ?? 0
    }
}
