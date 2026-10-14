import Foundation
import Operation_iOS

/// Persists what a recovery scan found, keeping rows the store already holds as they are.
protocol RecoveredAssetStoring: Sendable {
    func saveNew(coins: [Coin]) async throws
    func saveNew(vouchers: [Voucher]) async throws
}

/// A coin already known locally keeps its local state — a handoff mark or provenance a chain read
/// cannot see — so only rows the store has never held are written.
final class RecoveredAssetStore: RecoveredAssetStoring, @unchecked Sendable {
    private let databaseFactory: any DatabaseDependencyFactoring

    init(databaseFactory: any DatabaseDependencyFactoring) {
        self.databaseFactory = databaseFactory
    }

    func saveNew(coins: [Coin]) async throws {
        guard !coins.isEmpty else { return }
        let known = try await databaseFactory.makeCoinRepository(publicKeys: coins.map(\.publicKey))
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
        let knownIds = Set(known.map(\.identifier))
        let new = coins.filter { !knownIds.contains($0.identifier) }
        guard !new.isEmpty else { return }
        try await databaseFactory.makeCoinRepository().saveOperation({ new }, { [] }).asyncExecute()
    }

    func saveNew(vouchers: [Voucher]) async throws {
        guard !vouchers.isEmpty else { return }
        let known = try await databaseFactory.makeVoucherRepository(publicKeys: vouchers.map(\.publicKey))
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
        let knownIds = Set(known.map(\.identifier))
        let new = vouchers.filter { !knownIds.contains($0.identifier) }
        guard !new.isEmpty else { return }
        try await databaseFactory.makeVoucherRepository().saveOperation({ new }, { [] }).asyncExecute()
    }
}
