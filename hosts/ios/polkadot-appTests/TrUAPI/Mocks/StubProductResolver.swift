import Foundation
import Products
@testable import polkadot_app

/// Answers product resolution without a chain read.
final class StubProductResolver: ProductResolving, @unchecked Sendable {
    private let resolution: (ProductId) throws -> ResolvedProduct

    /// Resolves to the legacy shape by default, which keeps SPA tests on the base name exactly as
    /// they were before manifests existed.
    init(resolving: @escaping (ProductId) throws -> ResolvedProduct = { ResolvedProduct.legacy(id: $0) }) {
        resolution = resolving
    }

    /// Answers every id with one product, for a test whose subject is what the product publishes
    /// rather than which id it was asked about.
    convenience init(alwaysResolvingTo product: ResolvedProduct) {
        self.init { _ in product }
    }

    /// Fails every resolution, which is how a manifest that cannot be read reaches its caller.
    convenience init(alwaysFailing error: any Error) {
        self.init { _ in throw error }
    }

    func resolve(_ productId: ProductId) async throws -> ResolvedProduct {
        try resolution(productId)
    }
}
