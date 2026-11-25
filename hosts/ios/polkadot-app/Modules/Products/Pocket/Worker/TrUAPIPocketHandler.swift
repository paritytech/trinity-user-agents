import Foundation
import os
import Products
import TrUAPIHost

/// The live side of a face: the product's render stream, and the actions going
/// back to it.
protocol PocketFaceStreaming: Sendable {
    /// Faces the product draws for `key`, each replacing the last. The stream
    /// stays silent while the worker is unavailable.
    func renderFaces(for key: PocketCardKey) -> AsyncThrowingStream<RendererNode, Error>

    func send(action: String, payload: Data, for key: PocketCardKey)
}

/// The Pocket half of one product's worker: the faces it draws for the cards
/// the user holds, and the presses going back to it.
///
/// Started by ``ProductPocketService`` while the user holds at least one of
/// this product's cards, which is what asks for the worker. One handler serves
/// every card its product draws, so three of them on screen run one worker.
///
/// A render stream that ends or fails leaves the last face on screen and is
/// opened again; a worker that restarts gets a fresh stream.
final class TrUAPIPocketHandler: PocketFaceStreaming, Sendable {
    let productId: ProductId
    private let runtime: ProductWorkerRuntimeProvider
    private let logger: LoggerProtocol

    private let state = OSAllocatedUnfairLock(initialState: State())

    private struct State {
        var claim: NativeWorkerClaim?
        var following: Task<Void, Never>?
        /// The execution of the worker while it runs, which presses go to.
        var execution: TrUAPIWorkerOperations?
        var disposed = false
    }

    init(
        productId: ProductId,
        runtime: @escaping ProductWorkerRuntimeProvider,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productId = productId
        self.runtime = runtime
        self.logger = logger
    }

    /// Asks for the worker and returns at once. The core starts it, retries it
    /// and gives up on a start that hangs, so there is nothing to wait for here.
    func start() async throws {
        let host = try runtime()
        try state.withLock { state in
            guard !state.disposed, state.claim == nil else { throw CancellationError() }

            state.claim = host.claimWorker(productId: productId)
            state.following = Task { [weak self, productId] in
                for await worker in host.workerExecutionState(productId: productId) {
                    self?.state.withLock { $0.execution = worker.execution }
                }
            }
        }
    }

    func dispose() {
        let (claim, following) = state.withLock { state in
            state.disposed = true
            defer {
                state.claim = nil
                state.following = nil
                state.execution = nil
            }
            return (state.claim, state.following)
        }
        following?.cancel()
        claim?.release()
    }

    func renderFaces(for key: PocketCardKey) -> AsyncThrowingStream<RendererNode, Error> {
        let request = ProductRendererRenderRequest(context: .pocketCard(cardId: key.cardId.value), payload: Data())
        do {
            return try runtime().renderFollowingWorker(key.productId, request, logger: logger) { $0 }
        } catch {
            logger.error("[pocket] the worker stream for \(key.cardId.value) ended: \(error)")
            return AsyncThrowingStream { $0.finish() }
        }
    }

    func send(action: String, payload: Data, for key: PocketCardKey) {
        guard let execution = state.withLock({ $0.execution }) else { return }

        do {
            try execution.publishRendererAction(
                HostRendererActionSubscribeItem(
                    context: .pocketCard(cardId: key.cardId.value),
                    actionId: action,
                    payload: payload
                )
            )
        } catch {
            logger.error("[pocket] action '\(action)' for \(key.cardId.value) was not delivered: \(error)")
        }
    }
}
