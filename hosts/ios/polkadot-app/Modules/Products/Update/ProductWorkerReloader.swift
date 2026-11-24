import TrUAPIHost
import Foundation
import Products

/// Chat handlers and Pocket cards read the current execution per call, so a worker restart is the whole reload.
final class ProductWorkerReloader: ProductWorkerReloading {
    private let runtime: @Sendable () throws -> TrUAPIHostRuntime

    init(runtime: @escaping @Sendable () throws -> TrUAPIHostRuntime) {
        self.runtime = runtime
    }

    func reloadWorker(of productId: ProductId) async {
        try? runtime().reloadWorker(productId: productId)
    }
}
