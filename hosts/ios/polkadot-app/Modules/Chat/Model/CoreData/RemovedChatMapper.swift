import Foundation
import Operation_iOS
import CoreData
import SubstrateSdk

final class RemovedChatMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.RemovedChat
    typealias CoreDataEntity = CDRemovedChat
}

extension RemovedChatMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let identifier = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.identifier)
            )
        }

        let accountId = try Data(hexString: identifier)

        guard let removedAt = entity.removedAt else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.removedAt)
            )
        }

        return Chat.RemovedChat(
            accountId: accountId,
            removedAt: removedAt
        )
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.removedAt = model.removedAt
    }
}
