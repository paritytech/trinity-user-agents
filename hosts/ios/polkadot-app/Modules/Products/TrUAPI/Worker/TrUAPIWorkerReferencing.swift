import Foundation
import Products
import TrUAPIHost

/// The worker reference a modality holder keeps for as long as it needs the
/// product's worker: a card on screen, a chat session while it is open. The
/// core counts these and starts or stops the worker on the transitions across
/// zero.
protocol TrUAPIWorkerReferencing: Sendable {
    func acquireWorker(productId: ProductId)

    func releaseWorker(productId: ProductId)
}

extension TrUAPIHostRuntime: TrUAPIWorkerReferencing {}

/// What a handler holds of its product's worker request.
///
/// `asking` is the window between a handler asking and the core registering the
/// request. A dispose landing in it cannot give the request back, because the
/// release would reach the core first and drop the count of a worker another
/// holder is drawing from. The ask gives it back instead, once it returns.
enum WorkerRequest {
    case none
    case asking
    case held
}
