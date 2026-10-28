import Foundation

public enum DotNsSnapshotError: Error, Equatable {
    case recordNotSnapshotted(name: String, key: String)
    case contentHashNotSnapshotted(name: String)
    case archiveNotOnDisk
}

public struct SnapshotProductMetadataRepository: ProductMetadataRepositoryProtocol {
    private let store: ProductVersionStoring

    public init(store: ProductVersionStoring) {
        self.store = store
    }

    public func fetchMetadata(name: String, key: String) async throws -> String? {
        let version = store.version(of: ProductManifestRecords.baseName(of: name))

        return switch version?.records[ProductVersion.RecordKey(name: name, key: key)] {
        case let .value(record):
            record
        case .absent:
            nil
        case nil:
            throw DotNsSnapshotError.recordNotSnapshotted(name: name, key: key)
        }
    }
}

public struct SnapshotDotNsTldProvider: DotNsTldProviding {
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
