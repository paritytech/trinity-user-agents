import Foundation
import CoreData
import Operation_iOS
import SubstrateSdk

final class RecentContactMapper {
    var entityIdentifierFieldName: String { #keyPath(CDRecentContact.identifier) }

    private let decoder = JSONDecoder()
    private let encoder = JSONEncoder()

    typealias DataProviderModel = RecentContactModel
    typealias CoreDataEntity = CDRecentContact
}

extension RecentContactMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        try RecentContactModel(
            accountID: AccountId(hexString: entity.hexAccountID!),
            lastUsed: entity.lastUsed!,
            chainAssetID: ChainAssetId(
                chainId: entity.chainID!,
                assetId: UInt64(bitPattern: entity.assetID)
            )
        )
    }

    func populate(entity: CoreDataEntity, from model: DataProviderModel, using _: NSManagedObjectContext) throws {
        entity.identifier = model.identifier
        entity.hexAccountID = model.accountID.toHex()
        entity.lastUsed = model.lastUsed
        entity.chainID = model.chainAssetID.chainId
        entity.assetID = Int64(bitPattern: model.chainAssetID.assetId)
    }
}
