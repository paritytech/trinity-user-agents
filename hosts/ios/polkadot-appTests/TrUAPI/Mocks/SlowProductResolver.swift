import Foundation
import Products

/// Answers with `product` only after `delay`, and, like ``ProductResolver``, waits on a read shared
/// through an unstructured task, so cancelling the caller does not end the wait.
struct SlowProductResolver: ProductResolving {
    let product: ResolvedProduct
    let delay: Duration

    func resolve(_: ProductId) async throws -> ResolvedProduct {
        try await Task { [delay] in try await Task.sleep(for: delay) }.value
        return product
    }
}
