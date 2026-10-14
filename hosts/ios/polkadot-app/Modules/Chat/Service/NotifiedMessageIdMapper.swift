import CoreData
import Foundation
import Operation_iOS

final class NotifiedMessageIdMapper {
    var entityIdentifierFieldName: String { #keyPath(CDNotifiedMessageId.identifier) }

    typealias DataProviderModel = Chat.NotifiedMessageId
    typealias CoreDataEntity = CDNotifiedMessageId
}

extension NotifiedMessageIdMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        Chat.NotifiedMessageId(messageId: entity.identifier!)
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
    }
}
