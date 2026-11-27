import struct TrUAPIHost.ProductId
@testable import polkadot_app

final class StubProductWorkerReloader: ProductWorkerReloading {
    private(set) var reloadedIds: [ProductId] = []

    func reloadWorker(of productId: ProductId) {
        reloadedIds.append(productId)
    }
}
