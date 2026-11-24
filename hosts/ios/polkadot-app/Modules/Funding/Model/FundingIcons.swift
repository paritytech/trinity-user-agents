import struct TrUAPIHost.ProductId
import Foundation
import Kingfisher
@preconcurrency import Products
import UIKit

/// Which of a provider's funding icons is asked for.
enum FundingIconKind: String, Sendable {
    case network = "networks"
    case token = "tokens"
}

/// Where a provider ships its funding icons in its worker archive:
/// `funding/icons/networks/<network>.png` and `funding/icons/tokens/<symbol>.png`,
/// the id lowercased. Each is a square PNG with transparency.
enum FundingIconPath {
    /// Nil for an id that is empty or, lowercased, leaves `[a-z0-9-]`: no
    /// provider ships an icon for it.
    static func path(_ kind: FundingIconKind, id: String) -> String? {
        let name = id.lowercased()
        guard !name.isEmpty, name.allSatisfy(\.isFundingIconCharacter) else { return nil }

        return "funding/icons/\(kind.rawValue)/\(name).png"
    }
}

/// A network's or token's icon, read from the worker archive of the provider
/// whose routes or quote it appears in, never from another provider's. Loads
/// off the main thread through the app's image cache. Nil while the archive
/// ships none, and the screen keeps the monogram.
struct FundingIconLoader: Sendable {
    private let archive: ProductWorkerArchive
    private let workerArchive: @Sendable (ProductId) async -> ProductId?

    /// `workerArchive` names the archive a provider's worker is served from.
    init(archive: ProductWorkerArchive, workerArchive: @escaping @Sendable (ProductId) async -> ProductId?) {
        self.archive = archive
        self.workerArchive = workerArchive
    }

    func image(_ kind: FundingIconKind, id: String, provider: ProductId) async -> UIImage? {
        guard let url = await url(kind, id: id, provider: provider) else { return nil }

        return try? await KingfisherManager.shared.retrieveImage(with: url).image
    }

    /// The icon's file in the provider's worker archive, when it ships one.
    func url(_ kind: FundingIconKind, id: String, provider: ProductId) async -> URL? {
        guard let path = FundingIconPath.path(kind, id: id),
              let contentId = await workerArchive(provider),
              let url = await archive.url(contentId: contentId, path: path),
              FileManager.default.fileExists(atPath: url.path)
        else { return nil }

        return url
    }
}

extension FundingIconLoader {
    /// Reads each provider's worker archive as the Pocket reads a product's:
    /// the one already on disk, else the one its manifest names.
    static func make(
        dotNsResolver: any DotNsResolverProtocol,
        products: any ProductResolving
    ) -> FundingIconLoader {
        FundingIconLoader(archive: ProductWorkerArchive(dotNsResolver: dotNsResolver)) { provider in
            await PocketWorkerArchiveName(
                productId: provider,
                published: { try? await products.resolve(provider).contentId(for: .worker) }
            ).resolve()
        }
    }
}

extension FundingNetwork {
    /// The network's icon as `provider` ships it.
    func icon(from loader: FundingIconLoader, provider: ProductId) async -> UIImage? {
        await loader.image(.network, id: id, provider: provider)
    }
}

extension FundingToken {
    /// The token's icon as `provider` ships it.
    func icon(from loader: FundingIconLoader, provider: ProductId) async -> UIImage? {
        await loader.image(.token, id: symbol, provider: provider)
    }
}

private extension Character {
    var isFundingIconCharacter: Bool {
        self == "-" || (isASCII && (isLowercase || isNumber))
    }
}
