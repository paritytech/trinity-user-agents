import Foundation
import Operation_iOS
import CoreData
import SubstrateSdk

final class PolkadotSignInHostMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = PolkadotSignInHost
    typealias CoreDataEntity = CDPolkadotSignInHost
}

extension PolkadotSignInHostMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let accountId = try entity.identifier?.fromHex() else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.identifier)
            )
        }

        guard let publicKey = entity.publicKey else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.publicKey)
            )
        }

        guard let name = entity.name else {
            throw CoreDataMapperError.missingRequiredData(
                keyPath: #keyPath(CoreDataEntity.name)
            )
        }

        let iconUrl = entity.iconUrl.flatMap { URL(string: $0) }

        return PolkadotSignInHost(
            accountId: accountId,
            publicKey: publicKey,
            name: name,
            iconUrl: iconUrl
        )
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.publicKey = model.publicKey
        entity.name = model.name
        entity.iconUrl = model.iconUrl?.absoluteString
    }
}
