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

    /// Holds the worker of the product for one wake of 30 seconds (07-LATER §10.2). A product has one wake hold, and
    /// a new wake moves its end. The core releases the hold itself.
    func wakeProduct(productId: ProductId, reason: ChatWakeReason) throws -> ChatWakeOutcome

    /// Tells the core when the worker of the product runs. The core holds a `foreground` worker while the host
    /// reports the app in the foreground (VERBOSITY-WASM-PRODUCT I5).
    func setWorkerKeepAlive(productId: ProductId, keepAlive: WorkerKeepAlive)

    /// Gives a wake push of the push relay to the core. The core checks the push and wakes each product that watches
    /// its topic for 30 seconds (VERBOSITY-WASM-PRODUCT A1). It throws `PushWakeRejection` for a push it does not
    /// trust.
    func handleWakePush(payload: String) async throws -> [PushWakeDelivery]
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
