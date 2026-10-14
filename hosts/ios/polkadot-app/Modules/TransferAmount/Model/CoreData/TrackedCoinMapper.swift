import Foundation
import CoreData
import Coinage
import Operation_iOS

/// Maps a `CDCoin` to a ``TrackedCoin`` — the raw coin plus the durability overlay derived from its
/// input/output entry relations. Read-only: `populate` is unsupported.
final class TrackedCoinMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = TrackedCoin
    typealias CoreDataEntity = CDCoin

    private let coinMapper = CoinMapper()
    private let currentInstallation: CoinageCurrentInstallationContextReader

    /// The reader caches the installation id for the life of the process, so callers that map many
    /// rows or open many subscriptions share one instead of re-reading the row per mapper.
    init(currentInstallation: CoinageCurrentInstallationContextReader = .init()) {
        self.currentInstallation = currentInstallation
    }
}

extension TrackedCoinMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        let coin = try coinMapper.transform(entity: entity)
        return try TrackedCoin(
            coin: coin,
            state: CoinageAssetStateDeriver.state(
                handedOff: coin.handoffMark != .none,
                isRecovered: currentInstallation.isRecovered(coin.derivationIndex, of: entity),
                inputs: entity.coinageTxInputs,
                output: entity.coinageTxOutput
            )
        )
    }

    func populate(
        entity _: CoreDataEntity,
        from _: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        throw CoreDataMapperError.unsupported
    }
}
