import Foundation
import CoreData

extension ChatMessageEntityMapper {
    func groupRelatedMessages(
        entity: CDChatMessage,
        groupingId: String,
        context: NSManagedObjectContext
    ) throws {
        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = .siblingGroupMessages(groupingId: groupingId, excluding: entity)

        let siblings = try context.fetch(request)
        guard !siblings.isEmpty else { return }

        entity.addToRelatedMessages(NSSet(array: siblings))
    }

    func getRelatedMessages(from entity: CDChatMessage) -> [Chat.RelatedLocalMessage] {
        guard let related = entity.relatedMessages as? Set<CDChatMessage> else {
            return []
        }

        return related.compactMap { try? buildRelatedLocalMessage(from: $0) }
    }

    func buildRelatedLocalMessage(from entity: CDChatMessage) throws -> Chat.RelatedLocalMessage {
        guard let messageId = entity.messageId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDChatMessage.messageId)
            )
        }

        let statusRawValue = entity.status
        guard let status = Chat.LocalMessage.Status(rawValue: statusRawValue) else {
            throw MapperError.invalidStatus(statusRawValue)
        }

        let timestamp = UInt64(bitPattern: entity.timestamp)
        let content = try Self.getContent(from: entity)

        return Chat.RelatedLocalMessage(
            messageId: messageId,
            timestamp: timestamp,
            content: content,
            status: status
        )
    }
}
