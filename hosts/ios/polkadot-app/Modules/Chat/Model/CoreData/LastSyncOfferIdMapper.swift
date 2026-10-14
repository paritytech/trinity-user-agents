import Foundation
import CoreData
import Operation_iOS
import SubstrateSdk

extension Chat {
    struct LastSyncOfferIdUpdate: Equatable {
        let statementAccountId: Data
        let lastSyncOfferId: String?
    }
}

extension Chat.LastSyncOfferIdUpdate: Identifiable {
    var identifier: String {
        statementAccountId.toHex()
    }
}

final class LastSyncOfferIdMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CDLocalDevice.identifier)
    }

    typealias DataProviderModel = Chat.LastSyncOfferIdUpdate
    typealias CoreDataEntity = CDLocalDevice
}

extension LastSyncOfferIdMapper: CoreDataMapperProtocol {
    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.lastSyncOfferId = model.lastSyncOfferId
    }
}
