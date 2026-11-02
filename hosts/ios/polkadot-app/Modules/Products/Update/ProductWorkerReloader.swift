import Foundation
import Products

/// Chat handlers and Pocket cards read the current execution per call, so a worker restart is the whole reload.
final class ProductWorkerReloader: ProductWorkerReloading {
    private let truapiWorkers: @Sendable () -> (any TrUAPIWorkerManaging)?

    init(truapiWorkers: @escaping @Sendable () -> (any TrUAPIWorkerManaging)?) {
        self.truapiWorkers = truapiWorkers
    }

    func reloadWorker(of productId: ProductId) async {
        await truapiWorkers()?.reload(productId: productId)
    }
}
