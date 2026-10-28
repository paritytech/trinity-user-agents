import Foundation
import SDKLogger

/// What a product resolves to and which archive each executable serves, both readable with no
/// network. Manifests are cached per product and archives per executable, so one executable can
/// be opened or updated without touching the others.
public protocol ProductContentCaching: Sendable {
    func manifest(_ productId: ProductId) async throws -> ProductManifest
    /// Keeps the stored record of an executable whose applied archive came with it, except for
    /// `applyingContentId`: a record and its archive change together, through `apply`.
    func save(_ manifest: ProductManifest, applying applyingContentId: ProductId?) async
    /// Nil when nothing is applied under `contentId` or its archive is no longer on disk.
    func appliedArchive(of contentId: ProductId) async -> DotNsResolvedContent?
    func apply(_ archive: DotNsResolvedContent, record: ProductManifest.Record?, to contentId: ProductId) async
    func drop(_ contentId: ProductId, record: ProductManifest.Record?) async
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

    public func save(_ manifest: ProductManifest, applying applyingContentId: ProductId?) {
        let stored = store.records(of: manifest.product.id) ?? [:]

        save(manifest.records.filter { recordKey, _ in
            guard
                recordKey.key == ProductManifestRecords.executableKey,
                recordKey.name != applyingContentId,
                stored[recordKey] != nil
            else {
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

    public func apply(_ archive: DotNsResolvedContent, record: ProductManifest.Record?, to contentId: ProductId) {
        save(record, of: contentId)
        storage.contentHashes.putContentHash(name: contentId, hash: archive.contentHash)
    }

    public func drop(_ contentId: ProductId, record: ProductManifest.Record?) {
        save(record, of: contentId)
        storage.contentHashes.removeContentHash(name: contentId)
    }
}

private extension ProductContentCache {
    func save(_ record: ProductManifest.Record?, of contentId: ProductId) {
        guard let record else { return }

        save([ProductManifest.RecordKey(name: contentId, key: ProductManifestRecords.executableKey): record])
    }

    func save(_ records: [ProductManifest.RecordKey: ProductManifest.Record]) {
        do {
            try store.save(records)
        } catch {
            logger.error("Failed to store manifest records: \(error)")
        }
    }
}
