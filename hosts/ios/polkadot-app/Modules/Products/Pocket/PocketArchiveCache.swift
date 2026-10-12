import Foundation
import Products

/// Where a product's content is already unpacked on this device.
///
/// The same pair ``CompositeProductFileProvider`` reads a worker's files
/// through: the name's content hash, and the directory that hash was unpacked
/// into. Nil means nothing has been fetched for that name yet.
enum PocketArchiveCache {
    static let onDisk: @Sendable (ProductId) -> URL? = { productId in
        guard let hash = ContentHashCache.shared.getContentHash(name: productId) else { return nil }

        return DotNsContentStorage().getContentDirectory(contentHash: hash)
    }
}

/// Which archive a product's faces and their images are read from.
///
/// The subname the product's manifest publishes settles it, but reading that
/// manifest is a chain read. At a cold start with no network it cannot land,
/// and a card the host already keeps would lose every image in it with the
/// archive holding them unpacked on disk. The subname dotNS fixes by convention
/// needs no read, so it answers whenever its archive is already there.
struct PocketWorkerArchiveName: Sendable {
    private let conventional: ProductId
    private let published: @Sendable () async -> ProductId?
    private let onDisk: @Sendable (ProductId) -> URL?

    init(
        productId: ProductId,
        published: @escaping @Sendable () async -> ProductId?,
        onDisk: @escaping @Sendable (ProductId) -> URL? = PocketArchiveCache.onDisk
    ) {
        conventional = ProductManifestRecords.subname(
            base: ProductManifestRecords.baseName(of: productId.lowercased()),
            kind: .worker
        )
        self.published = published
        self.onDisk = onDisk
    }

    /// Nothing unpacked under the conventional subname is a worker this device
    /// has never fetched, and only the manifest can name where that one lives.
    func resolve() async -> ProductId? {
        if onDisk(conventional) != nil { return conventional }

        return await published()
    }
}
