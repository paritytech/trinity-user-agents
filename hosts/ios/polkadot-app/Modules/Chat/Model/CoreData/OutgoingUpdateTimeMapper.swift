import Foundation
import CoreData
import Operation_iOS
import SubstrateSdk

extension Chat {
    struct OutgoingUpdateTimeUpdate: Equatable {
        let statementAccountId: Data
        let outgoingUpdateTime: UInt64?
    }
}

extension Chat.OutgoingUpdateTimeUpdate: Identifiable {
    var identifier: String {
        statementAccountId.toHex()
    }
}

final class OutgoingUpdateTimeMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CDLocalDevice.identifier)
    }

    typealias DataProviderModel = Chat.OutgoingUpdateTimeUpdate
    typealias CoreDataEntity = CDLocalDevice
}

extension OutgoingUpdateTimeMapper: CoreDataMapperProtocol {
    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.outgoingUpdateTime = model.outgoingUpdateTime.map { NSNumber(value: $0) }
    }
}
