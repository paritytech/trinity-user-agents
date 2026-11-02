import Foundation
import TrUAPIHost

public enum ProductManifestCacheError: Error, Equatable {
    case recordNotCached(name: String, key: String)
}

public struct CachedProductMetadataRepository: ProductMetadataRepositoryProtocol {
    private let store: ProductManifestStoring

    public init(store: ProductManifestStoring) {
        self.store = store
    }

    public func fetchMetadata(name: String, key: String) async throws -> String? {
        let productId = try ProductId(name)
        let records = try store.records(of: productIdOwner(productId: productId))

        return switch records?[ProductManifest.RecordKey(name: productId, key: key)] {
        case let .value(record):
            record
        case .absent:
            nil
        case nil:
            throw ProductManifestCacheError.recordNotCached(name: name, key: key)
        }
    }
}
