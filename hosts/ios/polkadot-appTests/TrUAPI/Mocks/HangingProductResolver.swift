import Foundation
import Products

/// Never answers in any time a test waits for, and, like ``ProductResolver``, waits on a read shared
/// through an unstructured task, so cancelling the caller does not end the wait.
struct HangingProductResolver: ProductResolving {
    func resolve(_ productId: ProductId) async throws -> ResolvedProduct {
        try await Task { try await Task.sleep(for: .seconds(60)) }.value
        return ResolvedProduct.legacy(id: productId)
    }
}
