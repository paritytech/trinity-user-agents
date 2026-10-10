import Foundation
@preconcurrency import Products

/// A product's worker archive on disk: the one place a card's face and the
/// images inside it are read from.
///
/// The archive already unpacked answers without a chain read and without a
/// download, which is what lets a card drawn from a kept face keep its images
/// offline. The fetch is paid only for a product nothing has been read for yet,
/// or for a read that has to show what the product publishes now.
struct ProductWorkerArchive: PocketArchiveReading, Sendable {
    /// Which archive a read is answered from.
    enum Content: Sendable {
        /// Whatever this device already holds for the name.
        case onDisk
        /// What the name resolves to now. What the user approves has to be what
        /// the product ships, not a version of it this device kept.
        case current
    }

    private let content: Content
    private let cachedRoot: @Sendable (ProductId) -> URL?
    private let dotNsResolver: any DotNsResolverProtocol

    init(
        dotNsResolver: any DotNsResolverProtocol,
        content: Content = .onDisk,
        cachedRoot: @escaping @Sendable (ProductId) -> URL? = PocketArchiveCache.onDisk
    ) {
        self.dotNsResolver = dotNsResolver
        self.content = content
        self.cachedRoot = cachedRoot
    }

    /// The file `path` names inside `contentId`'s archive, or nil when the
    /// archive cannot be reached or the path would leave it.
    func url(contentId: ProductId, path: String) async -> URL? {
        guard let root = await root(of: contentId) else { return nil }

        return ContentArchivePath.inside(root, path: path)
    }

    func file(contentId: ProductId, path: String, maxBytes: Int) async throws -> Data {
        guard let file = await url(contentId: contentId, path: path) else {
            throw PocketPreviewError.notReachable(path)
        }

        let handle = try FileHandle(forReadingFrom: file)
        defer { try? handle.close() }

        // One byte past the bound: enough to tell a file that is too large from
        // one that exactly fills it, without holding either of them whole.
        let read = try handle.read(upToCount: maxBytes + 1) ?? Data()
        guard read.count <= maxBytes else { throw PocketPreviewError.tooLarge(bytes: read.count) }

        return read
    }

    private func root(of contentId: ProductId) async -> URL? {
        if case .onDisk = content, let cached = cachedRoot(contentId) { return cached }

        return try? await dotNsResolver.resolveToLocalURL(dotNsName: contentId)
    }
}
