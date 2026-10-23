import Foundation
import Products

struct UnwiredProductWorkerReloader: ProductWorkerReloading {
    let logger: LoggerProtocol

    func reloadWorker(of productId: ProductId) async {
        logger.debug("Worker reload for \(productId) is not wired yet")
    }
}
