import Foundation
import Operation_iOS
import StructuredConcurrency

/// Protocol defining coin persistence operations.
///
/// Local coin status is derived from the durability entry graph on read, so this service no longer
/// writes status. The only stored fact it persists beyond identity is `age` and `isOnchain`, both
/// owned by chain sync, written through `save`.
public protocol CoinServiceProtocol: Sendable {
    /// Fetch all coins paired with their derived durability overlay.
    func fetchAllTrackedCoins() async throws -> [TrackedCoin]

    /// Fetch only the coins with the given public keys — a filtered query, not the whole set. Empty
    /// `publicKeys` returns an empty set without touching the store.
    func fetchCoins(publicKeys: Set<PublicKey>) async throws -> Set<Coin>

    /// Save coins to persistent storage.
    func save(coins: [Coin]) async throws
}

public final class CoinService: @unchecked Sendable {
    private let databaseFactory: any DatabaseDependencyFactoring
    private let coinRepository: AnyDataProviderRepository<Coin>
    private let trackedCoinRepository: AnyDataProviderRepository<TrackedCoin>

    public init(databaseFactory: any DatabaseDependencyFactoring) {
        self.databaseFactory = databaseFactory
        coinRepository = databaseFactory.makeCoinRepository()
        trackedCoinRepository = databaseFactory.makeTrackedCoinRepository()
    }
}

extension CoinService: CoinServiceProtocol {
    public func fetchAllTrackedCoins() async throws -> [TrackedCoin] {
        try await trackedCoinRepository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
    }

    public func fetchCoins(publicKeys: Set<PublicKey>) async throws -> Set<Coin> {
        guard !publicKeys.isEmpty else { return [] }
        let coins = try await databaseFactory.makeCoinRepository(publicKeys: Array(publicKeys))
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
        return Set(coins)
    }

    public func save(coins: [Coin]) async throws {
        guard !coins.isEmpty else { return }
        try await coinRepository.saveOperation({ coins }, { [] }).asyncExecute()
    }
}
