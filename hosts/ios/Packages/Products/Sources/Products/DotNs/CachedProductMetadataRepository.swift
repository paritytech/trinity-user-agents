import Foundation

public enum ProductManifestCacheError: Error, Equatable {
    case recordNotCached(name: String, key: String)
}

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
