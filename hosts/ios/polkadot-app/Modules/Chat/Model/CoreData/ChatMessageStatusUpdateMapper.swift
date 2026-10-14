import Foundation
import CoreData
import Operation_iOS
import SubstrateSdk

final class ChatMessageStatusUpdateMapper {
    typealias DataProviderModel = Chat.ChatMessageStatusUpdate
    typealias CoreDataEntity = CDChatMessage

    var entityIdentifierFieldName: String {
        #keyPath(CDChatMessage.messageId)
    }
}

extension ChatMessageStatusUpdateMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let messageId = entity.messageId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDChatMessage.messageId)
            )
        }

        guard let status = Chat.LocalMessage.Status(rawValue: entity.status) else {
            throw ChatMessageEntityMapper.MapperError.invalidStatus(entity.status)
        }

        return .init(messageId: messageId, status: status)
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        switch model.status {
        case let .incoming(incoming):
            try handleIncomingStatus(
                entity: entity,
                from: model,
                incomingStatus: incoming,
                using: context
            )
        case .outgoing:
            try handleOutgoingStatus(
                entity: entity,
                from: model,
                using: context
            )
        }
    }
}

private extension ChatMessageStatusUpdateMapper {
    func handleIncomingStatus(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        incomingStatus: Chat.LocalMessage.Status.IncomingStatus,
        using context: NSManagedObjectContext
    ) throws {
        let seenRawValue = Chat.LocalMessage.Status.incoming(.seen).rawValue
        let isAlreadySeen = entity.status == seenRawValue
        let hasStatusChanged = entity.status != model.status.rawValue

        // Update the status if the message hasn't been marked as seen
        if !isAlreadySeen, hasStatusChanged {
            entity.status = model.status.rawValue
            entity.markModified()
            entity.touchParent()
        }

        guard !entity.isSystem else {
            return
        }

        let finalStatus = isAlreadySeen ? .seen : incomingStatus
        let existingUnread: CDChatUnreadMessage? = try context.first(for: .unreadMessage(for: model.messageId))

        switch finalStatus {
        case .new where existingUnread == nil:
            guard let chatEntity = entity.chat else {
                throw CoreDataMapperError.missingRequiredData(
                    keyPath: #keyPath(CDChatMessage.chat)
                )
            }
            let newUnread = try context.insertNew(CDChatUnreadMessage.self)
            newUnread.messageId = model.messageId
            newUnread.chat = chatEntity
        case .new:
            break
        case .seen:
            if let existingUnread {
                context.delete(existingUnread)
            }
        }
    }

    func handleOutgoingStatus(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        entity.status = model.status.rawValue

        // we can't have loops but still worth to protect against broken state
        var seenMessageIds: Set<Chat.MessageId> = []

        try propagateOutgoingStatusToCompactedChildren(
            parentEntity: entity,
            status: model.status,
            context: context,
            seenMessageIds: &seenMessageIds
        )
    }

    func propagateOutgoingStatusToCompactedChildren(
        parentEntity: CoreDataEntity,
        status: Chat.LocalMessage.Status,
        context: NSManagedObjectContext,
        seenMessageIds: inout Set<Chat.MessageId>
    ) throws {
        let compactedMessagesType = Chat.LocalMessage.Content.ContentType.compactedMessages.rawValue

        guard
            status.isOutgoing,
            let parentMessageId = parentEntity.messageId,
            parentEntity.contentType == compactedMessagesType else {
            return
        }

        seenMessageIds.insert(parentMessageId)

        let request = CDChatMessage.fetchRequest()
        request.predicate = .messagesByCompactionId(parentMessageId)

        let children = try context.fetch(request)

        guard !children.isEmpty else {
            return
        }

        for child in children {
            child.status = status.rawValue

            if let childId = child.messageId, !seenMessageIds.contains(childId) {
                try propagateOutgoingStatusToCompactedChildren(
                    parentEntity: child,
                    status: status,
                    context: context,
                    seenMessageIds: &seenMessageIds
                )
            }
        }
    }
}
