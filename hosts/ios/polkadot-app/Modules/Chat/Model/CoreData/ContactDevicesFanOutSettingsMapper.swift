import Foundation
import Operation_iOS
import CoreData
import SubstrateSdk

extension Chat {
    struct ContactDevicesFanOutSettings {
        let accountId: AccountId
        let pendingDevicesFanOut: Bool
    }
}

final class ContactDevicesFanOutSettingsMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.ContactDevicesFanOutSettings
    typealias CoreDataEntity = CDChatContact
}

extension ContactDevicesFanOutSettingsMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case missingContact
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.missingContact
        }

        entity.pendingDevicesFanOut = model.pendingDevicesFanOut
    }
}

extension Chat.ContactDevicesFanOutSettings: Identifiable {
    var identifier: String {
        accountId.toHex()
    }
}
