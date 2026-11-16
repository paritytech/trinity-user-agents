import struct TrUAPIHost.ProductId
import Foundation
@preconcurrency import Products

/// Fetches the archive a product's surface is served from, before anything
/// presses through to it.
///
/// The archive is the slow part of opening a product: a chain read and a
/// content fetch. Doing it ahead turns the press into a load from disk.
struct ProductArchiveWarmer: Sendable {
    private let products: any ProductResolving
    private let dotNsResolver: any DotNsResolverProtocol

    init(products: any ProductResolving, dotNsResolver: any DotNsResolverProtocol) {
        self.products = products
        self.dotNsResolver = dotNsResolver
    }

    func warm(_ productId: ProductId, serving strategy: ExecutableKindStrategy) async throws {
        let product = try await products.resolve(productId)
        let contentId = try product.contentId(for: product.servedKind(for: strategy))

        _ = try await dotNsResolver.resolveToLocalURL(dotNsName: contentId.rawValue)
    }
}
