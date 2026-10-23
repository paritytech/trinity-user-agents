import Foundation

public protocol ProductVersionApplying: Sendable {
    func apply(_ version: ProductVersion)
}

public struct ProductVersionApplier: ProductVersionApplying {
    private let snapshot: DotNsRecordSnapshotProtocol
    private let contentHashCache: ContentHashCacheProtocol

    public init(snapshot: DotNsRecordSnapshotProtocol, contentHashCache: ContentHashCacheProtocol) {
        self.snapshot = snapshot
        self.contentHashCache = contentHashCache
    }

    public func apply(_ version: ProductVersion) {
        for (recordKey, record) in version.records {
            snapshot.store(record, name: recordKey.name, key: recordKey.key)
        }

        for (name, hash) in version.contentHashes {
            contentHashCache.putContentHash(name: name, hash: hash)
        }
    }
}
