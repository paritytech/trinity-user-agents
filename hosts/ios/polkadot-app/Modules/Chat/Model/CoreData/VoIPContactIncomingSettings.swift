import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

extension Chat {
    struct VoIPContactIncomingSettings {
        let accountId: AccountId
        let voipPushToken: Data
    }
}

final class VoIPContactIncomingSettingsMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.VoIPContactIncomingSettings
    typealias CoreDataEntity = CDChatContact
}

extension VoIPContactIncomingSettingsMapper: CoreDataMapperProtocol {
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

        entity.voipPushToken = model.voipPushToken
    }
}

extension Chat.VoIPContactIncomingSettings: Identifiable {
    var identifier: String {
        accountId.toHex()
    }
}
