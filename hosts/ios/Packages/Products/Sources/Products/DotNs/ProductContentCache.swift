import Foundation
import SDKLogger

/// What a product resolves to and which archive each executable serves, both readable with no
/// network. Manifests are cached per product and archives per executable, so one executable can
/// be opened or updated without touching the others.
public protocol ProductContentCaching: Sendable {
    /// The product as applied: its root record and each executable's applied record.
    func manifest(_ productId: ProductId) async throws -> ProductManifest
    /// Nil when nothing is applied under `contentId` or its archive is no longer on disk.
    func appliedArchive(of contentId: ProductId) async -> DotNsResolvedContent?
    /// Takes the root record, and the records of executables nothing is applied for yet. An
    /// applied executable keeps its record until a release of it is applied.
    func adopt(_ manifest: ProductManifest) async
    /// Applies one executable's release: its record with its archive, or its removal when the
    /// archive is nil.
    func apply(_ release: ExecutableRelease) async
}

public actor ProductContentCache {
    private let store: ProductManifestStoring
    private let storage: DotNsLocalStorage
    private let tldProvider: DotNsTldProviding
    private let resolverFactory: ProductResolverFactory
    private let logger: SDKLoggerProtocol

    public init(
        store: ProductManifestStoring,
        storage: DotNsLocalStorage,
        tldProvider: DotNsTldProviding,
        resolverFactory: ProductResolverFactory,
        logger: SDKLoggerProtocol
    ) {
        self.store = store
        self.storage = storage
        self.tldProvider = tldProvider
        self.resolverFactory = resolverFactory
        self.logger = logger
    }
}

extension ProductContentCache: ProductContentCaching {
    public func manifest(_ productId: ProductId) async throws -> ProductManifest {
        let hostProvider = ProductHostFactory(tldProvider: CachedDotNsTldProvider(tldProvider: tldProvider))

        // The resolver treats an unresolvable host as a legacy product; with no known TLD that
        // would be a guess, so it is a cache miss instead.
        _ = try await hostProvider.resolveHost(rawString: productId.lowercased())

        return try await resolverFactory.makeResolver(
            metadataRepository: CachedProductMetadataRepository(store: store),
            hostProvider: hostProvider
        ).resolveProductManifest(productId)
    }

    public func adopt(_ manifest: ProductManifest) {
        let stored = store.records(of: manifest.product.id) ?? [:]

        save(manifest.records.filter { recordKey, _ in
            guard recordKey.key == ProductManifestRecords.executableKey, stored[recordKey] != nil else {
                return true
            }

            return appliedArchive(of: recordKey.name) == nil
        })
    }

    public func appliedArchive(of contentId: ProductId) -> DotNsResolvedContent? {
        guard
            let contentHash = storage.contentHashes.getContentHash(name: contentId),
            let localURL = storage.content.getContentDirectory(contentHash: contentHash)
        else {
            return nil
        }

        return DotNsResolvedContent(contentHash: contentHash, localURL: localURL)
    }

    public func apply(_ release: ExecutableRelease) {
        if let record = release.record {
            save([ProductManifest.RecordKey(
                name: release.contentId,
                key: ProductManifestRecords.executableKey
            ): record])
        }

        if let archive = release.archive {
            storage.contentHashes.putContentHash(name: release.contentId, hash: archive.contentHash)
        } else {
            storage.contentHashes.removeContentHash(name: release.contentId)
        }
    }
}

private extension ProductContentCache {
    func save(_ records: [ProductManifest.RecordKey: ProductManifest.Record]) {
        do {
            try store.save(records)
        } catch {
            logger.error("Failed to store manifest records: \(error)")
        }
    }
}
