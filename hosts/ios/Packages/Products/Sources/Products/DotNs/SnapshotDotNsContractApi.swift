import Foundation
import SubstrateSdkExt

public enum DotNsSnapshotError: Error, Equatable {
    case recordNotSnapshotted(name: String, key: String)
    case contentHashNotSnapshotted(name: String)
    case archiveNotOnDisk
}

public final class SnapshotDotNsContractApi: DotNsContractApiProtocol {
    private let snapshot: DotNsRecordSnapshotProtocol
    private let contentHashCache: ContentHashCacheProtocol

    public init(snapshot: DotNsRecordSnapshotProtocol, contentHashCache: ContentHashCacheProtocol) {
        self.snapshot = snapshot
        self.contentHashCache = contentHashCache
    }

    public func resolveContentHash(dotNsName: String) async throws -> Data {
        guard let hash = contentHashCache.getContentHash(name: dotNsName) else {
            throw DotNsSnapshotError.contentHashNotSnapshotted(name: dotNsName)
        }

        return try hash.fromHex()
    }

    public func getMetadata(dotNsName: String, key: String) async throws -> String? {
        switch snapshot.record(name: dotNsName, key: key) {
        case let .value(value):
            return value
        case .absent:
            return nil
        case nil:
            throw DotNsSnapshotError.recordNotSnapshotted(name: dotNsName, key: key)
        }
    }

    public func clearCache() {
        snapshot.clearAll()
    }
}

public struct SnapshotCarFetcher: CarFetcherProtocol {
    public init() {}

    public func fetchCarToFile(
        contentHash _: Data,
        onProgress _: @escaping (_ downloaded: Int64, _ total: Int64?) -> Void
    ) async throws -> URL {
        throw DotNsSnapshotError.archiveNotOnDisk
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
