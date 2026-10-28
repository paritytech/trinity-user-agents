import Foundation

public enum ProductManifestCacheError: Error, Equatable {
    case recordNotCached(name: String, key: String)
}

/// Answers manifest record reads from the records a product was last resolved from.
public struct CachedProductMetadataRepository: ProductMetadataRepositoryProtocol {
    private let store: ProductManifestStoring

    public init(store: ProductManifestStoring) {
        self.store = store
    }

    public func fetchMetadata(name: String, key: String) async throws -> String? {
        let records = store.records(of: ProductManifestRecords.baseName(of: name))

        return switch records?[ProductManifest.RecordKey(name: name, key: key)] {
        case let .value(record):
            record
        case .absent:
            nil
        case nil:
            throw ProductManifestCacheError.recordNotCached(name: name, key: key)
        }
    }
}

/// Answers with the TLD already known on the device and never reads it from the chain.
public struct CachedDotNsTldProvider: DotNsTldProviding {
    private let tldProvider: DotNsTldProviding

    public init(tldProvider: DotNsTldProviding) {
        self.tldProvider = tldProvider
    }

    public func currentTld() -> String? {
        tldProvider.currentTld()
    }

    public func resolveTld() async throws -> String {
        guard let tld = tldProvider.currentTld() else {
            throw DotNsContractError.tldNotFound
        }

        return tld
    }

    public func refresh() {
        tldProvider.refresh()
    }

    public func reset() {
        tldProvider.reset()
    }
}
