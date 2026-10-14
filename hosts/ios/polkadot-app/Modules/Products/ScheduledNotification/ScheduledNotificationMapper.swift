import CoreData
import Operation_iOS

final class ScheduledNotificationMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ScheduledNotificationEntry
    typealias CoreDataEntity = CDScheduledNotification
}

extension ScheduledNotificationMapper: CoreDataMapperProtocol {
    func transform(entity: CDScheduledNotification) throws -> ScheduledNotificationEntry {
        guard let productId = entity.productId else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CDScheduledNotification.productId)
            )
        }

        return ScheduledNotificationEntry(
            productId: productId,
            notificationId: UInt32(bitPattern: entity.notificationId)
        )
    }

    func populate(
        entity: CDScheduledNotification,
        from model: ScheduledNotificationEntry,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.productId = model.productId
        entity.notificationId = Int32(bitPattern: model.notificationId)
    }
}
