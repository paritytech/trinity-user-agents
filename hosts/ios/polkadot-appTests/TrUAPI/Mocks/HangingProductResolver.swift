import Foundation
import Products

/// Never answers in any time a test waits for, which is how a chain read that hangs reaches its
/// caller. The wait gives up when cancelled, so a caller that stops waiting lets the test end.
struct HangingProductResolver: ProductResolving {
    func resolve(_ productId: ProductId) async throws -> ResolvedProduct {
        try await Task.sleep(for: .seconds(60))
        return ResolvedProduct.legacy(id: productId)
    }
}
