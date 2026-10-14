import Foundation
import Operation_iOS
import CoreData
import SubstrateSdk

final class LocalDeviceMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.LocalDevice
    typealias CoreDataEntity = CDLocalDevice
}

extension LocalDeviceMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let identifier = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.identifier)
            )
        }

        let statementAccountId = try Data(hexString: identifier)

        guard let encryptionPublicKey = entity.encryptionPublicKey else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.encryptionPublicKey)
            )
        }

        guard let hostName = entity.hostName else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.hostName)
            )
        }

        guard let createdAt = entity.createdAt else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.createdAt)
            )
        }

        let outgoingUpdateTime = entity.outgoingUpdateTime?.uint64Value

        return DataProviderModel(
            statementAccountId: statementAccountId,
            encryptionPublicKey: encryptionPublicKey,
            hostName: hostName,
            createdAt: createdAt,
            hostVersion: entity.hostVersion,
            osType: entity.osType,
            osVersion: entity.osVersion,
            outgoingUpdateTime: outgoingUpdateTime,
            lastSyncOfferId: entity.lastSyncOfferId
        )
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.statementAccountId = model.statementAccountId
        entity.encryptionPublicKey = model.encryptionPublicKey
        entity.hostName = model.hostName
        entity.hostVersion = model.hostVersion
        entity.osType = model.osType
        entity.osVersion = model.osVersion
        entity.createdAt = model.createdAt
        entity.outgoingUpdateTime = model.outgoingUpdateTime.map { NSNumber(value: $0) }
        entity.lastSyncOfferId = model.lastSyncOfferId
    }
}
