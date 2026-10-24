import Foundation
import FoundationExt
import SDKLogger

public protocol ProductVersionCaching: Sendable {
    func open(_ productId: ProductId) async throws -> ProductContent
    func apply(_ version: ProductVersion) async
    func invalidate(_ version: ProductVersion) async
    func removeSupersededContent(keeping pinnedHashes: Set<String>) async
}

public actor ProductVersionCache {
    private let snapshot: DotNsRecordSnapshotProtocol
    private let storage: DotNsLocalStorage
    private let tldProvider: DotNsTldProviding
    private let resolverFactory: ProductResolverFactory
    private let dateProvider: DateProviding

    private enum Constants {
        static let minimumAgeBeforeRemoval: TimeInterval = 10 * 60
    }

    public init(
        snapshot: DotNsRecordSnapshotProtocol,
        storage: DotNsLocalStorage,
        tldProvider: DotNsTldProviding,
        resolverFactory: ProductResolverFactory,
        dateProvider: DateProviding = NowDateProvider()
    ) {
        self.snapshot = snapshot
        self.storage = storage
        self.tldProvider = tldProvider
        self.resolverFactory = resolverFactory
        self.dateProvider = dateProvider
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

    public func removeSupersededContent(keeping pinnedHashes: Set<String>) async {
        let now = await dateProvider.read()
        let keptHashes = pinnedHashes.union(storage.contentHashes.allContentHashes())

        let superseded = storage.content.storedContents().filter { content in
            !keptHashes.contains(content.contentHash)
                && now.timeIntervalSince(content.modifiedAt) >= Constants.minimumAgeBeforeRemoval
        }

        for content in superseded {
            do {
                try storage.content.deleteContent(contentHash: content.contentHash)
            } catch {
                resolverFactory.logger.error("Failed to remove superseded product content: \(error)")
            }
        }

        if !superseded.isEmpty {
            resolverFactory.logger.debug("Removed \(superseded.count) superseded product archives")
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
