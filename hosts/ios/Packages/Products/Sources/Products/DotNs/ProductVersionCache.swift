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
    private let resolverFactory: ProductResolverFactory


    public init(
        snapshot: DotNsRecordSnapshotProtocol,
        storage: DotNsLocalStorage,
        tldProvider: DotNsTldProviding,
        resolverFactory: ProductResolverFactory
    ) {
        self.snapshot = snapshot
        self.storage = storage
        self.tldProvider = tldProvider
        self.resolverFactory = resolverFactory
    }
}

extension ProductVersionCache: ProductVersionCaching {
    public func open(_ productId: ProductId) async throws -> ProductContent {
        let hostProvider = ProductHostFactory(tldProvider: SnapshotDotNsTldProvider(tldProvider: tldProvider))
        _ = try await hostProvider.resolveHost(rawString: productId.lowercased())

        let manifest = try await resolverFactory.makeResolver(
            metadataRepository: SnapshotProductMetadataRepository(snapshot: snapshot),
            hostProvider: hostProvider
        ).resolveProductManifest(productId)
        let resolved = manifest.product

        let appContent = try content(of: resolved.appContentId)
        var contents = [resolved.appContentId: appContent]

        let optionalIds = [resolved.executables.widget?.identifier, resolved.executables.worker?.identifier]
        for contentId in optionalIds.compactMap({ $0 }) where contents[contentId] == nil {
            contents[contentId] = try? content(of: contentId)
        }

        return ProductContent(
            resolved: resolved,
            contents: contents,
            appContent: appContent,
            version: ProductVersion(
                productId: resolved.id,
                records: manifest.records,
                contentHashes: contents.mapValues(\.contentHash)
            ),
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
