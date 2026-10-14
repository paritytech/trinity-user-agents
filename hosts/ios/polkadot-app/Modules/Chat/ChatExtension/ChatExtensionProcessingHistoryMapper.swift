import Foundation
import Operation_iOS
import CoreData

final class ChatExtensionProcessingHistoryMapper {
    typealias DataProviderModel = ChatExtension.ProcessingHistory
    typealias CoreDataEntity = CDChatExtensionHistory

    var entityIdentifierFieldName: String {
        #keyPath(CDChatExtensionHistory.identifier)
    }
}

extension ChatExtensionProcessingHistoryMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let identifier = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDChatExtensionHistory.identifier)
            )
        }

        guard let chatId = entity.chatId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDChatExtensionHistory.chatId)
            )
        }

        guard let extensionId = entity.extensionId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDChatExtensionHistory.extensionId)
            )
        }

        return ChatExtension.ProcessingHistory(
            messageId: identifier,
            chatId: chatId,
            extensionId: extensionId
        )
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.chatId = model.chatId
        entity.extensionId = model.extensionId
    }
}

extension ChatExtension.ProcessingHistory: Identifiable {
    var identifier: String {
        messageId
    }
}
