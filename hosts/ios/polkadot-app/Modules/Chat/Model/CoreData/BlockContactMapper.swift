import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

extension Chat {
    struct ContactBlockStatus {
        let accountId: AccountId
        let isBlocked: Bool
    }
}

final class BlockContactMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.ContactBlockStatus
    typealias CoreDataEntity = CDChatContact
}

extension BlockContactMapper: CoreDataMapperProtocol {
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

        entity.isBlocked = model.isBlocked
    }
}

extension Chat.ContactBlockStatus: Identifiable {
    var identifier: String {
        accountId.toHex()
    }
}
