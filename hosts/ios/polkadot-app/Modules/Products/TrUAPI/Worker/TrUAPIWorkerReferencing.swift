import Products
import TrUAPIHost

protocol TrUAPIWorkerClaiming: Sendable {
    func release()
}

extension NativeWorkerClaim: TrUAPIWorkerClaiming {}

protocol TrUAPIWorkerReferencing: Sendable {
    func claim(_ productId: ProductId) -> any TrUAPIWorkerClaiming
    func workerExited(productId: ProductId)
}

extension TrUAPIHostRuntime: TrUAPIWorkerReferencing {
    func claim(_ productId: ProductId) -> any TrUAPIWorkerClaiming {
        claimWorker(productId: productId)
    }
}
