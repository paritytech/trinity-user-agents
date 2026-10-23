import Foundation
import SDKLogger

public protocol ProductVersionCaching: Sendable {
    func open(_ productId: ProductId) async throws -> ProductContent
    func apply(_ version: ProductVersion) async
    func invalidate(_ version: ProductVersion) async
}

public actor ProductVersionCache {
    private let snapshot: DotNsRecordSnapshotProtocol
    private let storage: DotNsLocalStorage
    private let tldProvider: DotNsTldProviding
    private let logger: SDKLoggerProtocol

    public init(
        snapshot: DotNsRecordSnapshotProtocol,
        storage: DotNsLocalStorage,
        tldProvider: DotNsTldProviding,
        logger: SDKLoggerProtocol
    ) {
        self.snapshot = snapshot
        self.storage = storage
        self.tldProvider = tldProvider
        self.logger = logger
    }
}

extension ProductVersionCache: ProductVersionCaching {
    public func open(_ productId: ProductId) async throws -> ProductContent {
        let hostProvider = ProductHostFactory(tldProvider: SnapshotDotNsTldProvider(tldProvider: tldProvider))
        _ = try await hostProvider.resolveHost(rawString: productId.lowercased())

        let manifest = try await ProductResolver(
            metadataRepository: SnapshotProductMetadataRepository(snapshot: snapshot),
            hostProvider: hostProvider,
            logger: logger
        ).resolveProductManifest(productId)
        let resolved = manifest.product

        let appContent = try content(of: resolved.appContentId)
        var contentHashes = [resolved.appContentId: appContent.contentHash]

        if let worker = resolved.executables.worker, let workerContent = try? content(of: worker.identifier) {
            contentHashes[worker.identifier] = workerContent.contentHash
        }

        return ProductContent(
            resolved: resolved,
            appContent: appContent,
            version: ProductVersion(productId: resolved.id, records: manifest.records, contentHashes: contentHashes),
            source: .cache
        )
    }

    public func apply(_ version: ProductVersion) {
        for (recordKey, record) in version.records {
            snapshot.store(record, name: recordKey.name, key: recordKey.key)
        }

        for (name, hash) in version.contentHashes {
            storage.contentHashes.putContentHash(name: name, hash: hash)
        }
    }

    public func invalidate(_ version: ProductVersion) {
        for recordKey in version.records.keys {
            snapshot.remove(name: recordKey.name, key: recordKey.key)
        }
    }
}

private extension ProductVersionCache {
    func content(of name: String) throws -> DotNsResolvedContent {
        guard let contentHash = storage.contentHashes.getContentHash(name: name) else {
            throw DotNsSnapshotError.contentHashNotSnapshotted(name: name)
        }

        guard let localURL = storage.content.getContentDirectory(contentHash: contentHash) else {
            throw DotNsSnapshotError.archiveNotOnDisk
        }

        return DotNsResolvedContent(contentHash: contentHash, localURL: localURL)
    }
}
