import Foundation
import SDKLogger

public protocol ProductVersionCaching: Sendable {
    func open(_ productId: ProductId) async throws -> ProductContent
    func apply(_ version: ProductVersion) async
    func invalidate(_ version: ProductVersion) async
}

public actor ProductVersionCache {
    private let store: ProductVersionStoring
    private let storage: DotNsLocalStorage
    private let tldProvider: DotNsTldProviding
    private let resolverFactory: ProductResolverFactory

    public init(
        store: ProductVersionStoring,
        storage: DotNsLocalStorage,
        tldProvider: DotNsTldProviding,
        resolverFactory: ProductResolverFactory
    ) {
        self.store = store
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
            metadataRepository: SnapshotProductMetadataRepository(store: store),
            hostProvider: hostProvider
        ).resolveProductManifest(productId)
        let resolved = manifest.product
        let contentHashes = store.version(of: resolved.id)?.contentHashes ?? [:]

        let appContent = try content(of: resolved.appContentId, in: contentHashes)
        var contents = [resolved.appContentId: appContent]

        let optionalIds = [resolved.executables.widget?.identifier, resolved.executables.worker?.identifier]
        for contentId in optionalIds.compactMap({ $0 }) where contents[contentId] == nil {
            contents[contentId] = try? content(of: contentId, in: contentHashes)
        }

        return ProductContent(resolved: resolved, records: manifest.records, contents: contents)
    }

    public func apply(_ version: ProductVersion) {
        do {
            try store.save(version)
        } catch {
            resolverFactory.logger.error("Failed to store version of \(version.productId): \(error)")
        }

        for (name, hash) in version.contentHashes {
            storage.contentHashes.putContentHash(name: name, hash: hash)
        }
    }

    public func invalidate(_ version: ProductVersion) {
        store.remove(productId: version.productId)
    }
}

private extension ProductVersionCache {
    func content(of name: String, in contentHashes: [String: String]) throws -> DotNsResolvedContent {
        guard let contentHash = contentHashes[name] else {
            throw DotNsSnapshotError.contentHashNotSnapshotted(name: name)
        }

        guard let localURL = storage.content.getContentDirectory(contentHash: contentHash) else {
            throw DotNsSnapshotError.archiveNotOnDisk
        }

        return DotNsResolvedContent(contentHash: contentHash, localURL: localURL)
    }
}
