import Foundation
import CoreData
import Coinage
import Operation_iOS

/// Writes only the chain-sync presence fields (`age`, `isOnchain`) onto an existing `CDCoin`,
/// leaving every other column untouched. Write-only: it never reads a coin back.
final class CoinPresenceMapper {
    enum MappingError: Error {
        case missingCoin
    }

    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = CoinPresenceUpdate
    typealias CoreDataEntity = CDCoin
}

extension CoinPresenceMapper: CoreDataMapperProtocol {
    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.missingCoin
        }

        entity.age = model.age.map { NSNumber(value: $0) }
        entity.isOnchain = model.isOnchain
    }
}
