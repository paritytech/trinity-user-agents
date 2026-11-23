import struct TrUAPIHost.ProductId
import Foundation
import Products

/// Chat handlers and Pocket cards read the current execution per call, so a worker restart is the whole reload.
final class ProductWorkerReloader: ProductWorkerReloading {
    private let workers: any TrUAPIWorkerManaging

    init(workers: any TrUAPIWorkerManaging) {
        self.workers = workers
    }

    func reloadWorker(of productId: ProductId) async {
        await workers.reload(productId: productId)
    }
}
