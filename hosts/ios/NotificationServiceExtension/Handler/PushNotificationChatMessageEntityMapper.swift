import Foundation
import CoreData
import Operation_iOS
import SubstrateSdk

/// If the main app target has already persisted the message (e.g. it was running in the foreground),
/// the corresponding entity will already exist with a non-nil `messageId`. In that case we skip
/// population entirely so we do not re-apply chat or status updates that the main target has
/// already handled.
final class PushNotificationChatMessageEntityMapper {
    typealias DataProviderModel = Chat.LocalMessage
    typealias CoreDataEntity = CDChatMessage

    private let baseMapper = ChatMessageEntityMapper()

    var entityIdentifierFieldName: String {
        baseMapper.entityIdentifierFieldName
    }
}

extension PushNotificationChatMessageEntityMapper: CoreDataMapperProtocol {
    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.messageId == nil else {
            return
        }

        try baseMapper.populate(entity: entity, from: model, using: context)
    }
}
