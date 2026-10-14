import Foundation
import Keystore_iOS
import SubstrateSdk

/// ``CoinageCurrentInstallationStoring`` over the database row that names the current installation and
/// Keychain counters scoped by that installation.
///
/// The row lives and dies with the coin rows: a database lost on this device (a same-device restore
/// brings the Keychain back but not the backup-excluded store) takes the installation with it, a new
/// one starts, and the old one comes back through recovery like any previous installation — instead
/// of a surviving Keychain installation whose items 0..<N would never be scanned. The row never changes
/// once created, so the id is read once and kept for the life of the process. Each counter is a SCALE
/// `UInt64` holding the next unissued item; the actor serialises every operation, so the two
/// allocators can never hand out one item twice.
public actor CoinageCurrentInstallationStore: CoinageCurrentInstallationStoring {
    private let repository: any CoinageCurrentInstallationRepositoryProtocol
    private let keystore: KeystoreProtocol
    private let tags: any CoinageInstallationKeychainTagProviding
    private var current: Task<CoinageInstallationId, Error>?

    public init(
        repository: any CoinageCurrentInstallationRepositoryProtocol,
        keystore: KeystoreProtocol,
        tags: any CoinageInstallationKeychainTagProviding
    ) {
        self.repository = repository
        self.keystore = keystore
        self.tags = tags
    }

    public func getOrCreateCurrent() async throws -> CoinageInstallationId {
        let load = current ?? Task { [repository] in
            try await repository.getOrCreateCurrent { try CoinageInstallationId.random() }
        }
        current = load

        do {
            return try await load.value
        } catch {
            // Only a loaded id is kept: a read that failed is tried again by the next caller.
            if current == load { current = nil }
            throw error
        }
    }

    public func nextCoinItem() async throws -> DerivationIndex {
        try await reserveNextItem(tag: tags.coinIndexTag(for: getOrCreateCurrent()))
    }

    public func nextVoucherItem() async throws -> DerivationIndex {
        try await reserveNextItem(tag: tags.voucherIndexTag(for: getOrCreateCurrent()))
    }
}

private extension CoinageCurrentInstallationStore {
    func reserveNextItem(tag: String) throws -> DerivationIndex {
        let item = try storedNextItem(tag: tag) ?? 0
        guard item < DerivationIndex.max else {
            throw CoinageCurrentInstallationStoreError.counterExhausted(tag)
        }

        try keystore.saveKey((item + 1).scaleEncoded(), with: tag)
        return item
    }

    func storedNextItem(tag: String) throws -> DerivationIndex? {
        guard try keystore.checkKey(for: tag) else {
            return nil
        }

        let record = try keystore.fetchKey(for: tag)
        do {
            let decoder = try ScaleDecoder(data: record)
            let item = try DerivationIndex(scaleDecoder: decoder)
            guard decoder.remained == 0 else {
                throw CoinageCurrentInstallationStoreError.corruptedRecord(tag)
            }
            return item
        } catch {
            throw CoinageCurrentInstallationStoreError.corruptedRecord(tag)
        }
    }
}

public enum CoinageCurrentInstallationStoreError: Error, Equatable {
    case corruptedRecord(String)
    case counterExhausted(String)
}
