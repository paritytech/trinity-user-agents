import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

extension Chat {
    struct ContactIncomingSettings {
        let accountId: AccountId
        let pushToken: Data
        let peerPlatform: Chat.PeerPlatform
    }
}

final class ContactIncomingSettingsMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.ContactIncomingSettings
    typealias CoreDataEntity = CDChatContact
}

extension ContactIncomingSettingsMapper: CoreDataMapperProtocol {
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

        entity.pushToken = model.pushToken
        entity.pushPlatform = model.peerPlatform.rawValue
    }
}

extension Chat.ContactIncomingSettings: Identifiable {
    var identifier: String {
        accountId.toHex()
    }
}
