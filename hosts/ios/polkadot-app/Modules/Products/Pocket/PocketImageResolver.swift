import Foundation
import PolkadotUI
@preconcurrency import Products

/// Turns an image source inside a face into an address the image loader can
/// fetch: a file in the product's own archive, or a bulletin gateway address.
///
/// Nothing here throws. A source that cannot be resolved answers nil, and the
/// rest of the face still draws with a hole where the image would be. A card
/// whose product ships a bad path is worth more than no card at all.
struct PocketImageResolver: Sendable {
    /// Resolved per call: the archive a face is drawn from is the product's
    /// worker, and naming it may cost a manifest read.
    private let contentId: @Sendable () async -> ProductId?
    private let archive: ProductWorkerArchive
    private let ipfsUrl: @Sendable (String) -> URL?

    init(
        contentId: @escaping @Sendable () async -> ProductId?,
        archive: ProductWorkerArchive,
        ipfsUrl: @escaping @Sendable (String) -> URL?
    ) {
        self.contentId = contentId
        self.archive = archive
        self.ipfsUrl = ipfsUrl
    }

    func resolve(_ source: CustomMessageWidgetNode.ImageSource) async -> URL? {
        switch source {
        case let .bulletin(cid): ipfsUrl(cid)
        case let .archive(path): await archived(path)
        }
    }

    /// The path is written by the product, so the archive checks it stays
    /// inside the one it belongs to rather than trusting it.
    private func archived(_ path: String) async -> URL? {
        guard let contentId = await contentId() else { return nil }

        return await archive.url(contentId: contentId, path: path)
    }
}
