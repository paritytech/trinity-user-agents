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

    /// Warms the archive `kind` is served from, which is not the same name for
    /// every surface: a Pocket card opens the widget where a tab opens the app.
    func warm(_ productId: ProductId, serving kind: ExecutableKind) async throws {
        let contentId = try await products.resolve(productId).contentId(for: kind)

        _ = try await dotNsResolver.resolveToLocalURL(dotNsName: contentId)
    }
}
