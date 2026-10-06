import Foundation
import os
import Products
import TrUAPIHost

/// The live side of a face: the product's render stream, and the actions going
/// back to it.
protocol PocketFaceStreaming: Sendable {
    /// Faces the product draws for `key`, each replacing the last. Iterating
    /// holds one worker reference for the card's product, and the stream stays
    /// silent while the worker is unavailable.
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
final class TrUAPIPocketHandler: PocketFaceStreaming, @unchecked Sendable {
    private enum Retry {
        /// A worker that has just booted is not yet listening, so the first
        /// renders are expected to be refused.
        static let connectWindow = Duration.seconds(10)
        static let connectDelay = Duration.milliseconds(250)

        /// Doubling from a second up to half a minute: a worker that is simply
        /// slow is picked up at once, and a broken one is not asked on a loop
        /// for as long as its card is on screen.
        static let reopenDelay = Duration.seconds(1)
        static let maxReopenDelay = Duration.seconds(30)
        static let maxBackoffDoublings = 5
    }

    private let productId: ProductId
    private let workers: any TrUAPIWorkerManaging
    private let logger: LoggerProtocol

    /// What this handler holds of the product's worker request, so a dispose
    /// gives back exactly what the start asked for and never more.
    private let state = OSAllocatedUnfairLock(initialState: State())

    private struct State {
        var request = WorkerRequest.none
        var disposed = false
    }

    init(
        productId: ProductId,
        workers: any TrUAPIWorkerManaging,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productId = productId
        self.workers = workers
        self.logger = logger
    }

    /// Asks for the product's worker and returns once it is up, so the first
    /// card to draw finds something to ask.
    ///
    /// The request is ours from the moment we ask, including when the ask
    /// fails, so ``dispose()`` gives it back either way.
    func start() async throws {
        state.withLock { $0.request = .asking }

        do {
            _ = try await workers.ensureWorker(for: productId)
        } catch {
            settleRequest()
            throw error
        }
        settleRequest()
    }

    func dispose() {
        let release = state.withLock { state -> Bool in
            state.disposed = true
            guard state.request == .held else { return false }

            state.request = .none
            return true
        }
        guard release else { return }

        // A release, not a close. A chat session with the same product may
        // still be asking for the worker, and the core stops it once the last
        // request goes.
        workers.releaseWorker(for: productId)
    }

    /// The ask has returned, so the request is ours either way: it is
    /// registered before the wait that can fail. A dispose that landed while we
    /// were asking left the giving back to here.
    private func settleRequest() {
        let release = state.withLock { state -> Bool in
            guard state.disposed else {
                state.request = .held
                return false
            }

            state.request = .none
            return true
        }
        guard release else { return }

        workers.releaseWorker(for: productId)
    }

    func renderFaces(for key: PocketCardKey) -> AsyncThrowingStream<RendererNode, Error> {
        AsyncThrowingStream { continuation in
            let task = Task { await stream(key, into: continuation) }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    func send(action: String, payload: Data, for key: PocketCardKey) {
        guard let execution = workers.currentExecution(of: key.productId) else { return }

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

private extension TrUAPIPocketHandler {
    func stream(_ key: PocketCardKey, into continuation: AsyncThrowingStream<RendererNode, Error>.Continuation) async {
        await followExecutions(of: key, into: continuation)
        continuation.finish()
    }

    /// One render per execution the manager publishes: a worker that
    /// restarts is picked up as a new execution, and its stream replaces the
    /// one before it.
    func followExecutions(
        of key: PocketCardKey,
        into continuation: AsyncThrowingStream<RendererNode, Error>.Continuation
    ) async {
        var opened: Task<Void, Never>?
        var current: (any TrUAPIProductExecutionProtocol)?
        defer { opened?.cancel() }

        do {
            for try await execution in workers.executions(of: key.productId) {
                // The manager publishes every product's execution together,
                // so this re-sends whenever any other worker starts or stops.
                // Reopening on those would tear down a live render stream, and
                // pay the connect retries again, for a worker that never moved.
                guard execution !== current else { continue }
                current = execution

                opened?.cancel()
                guard let execution else { continue }

                opened = Task { await reopening(execution, for: key, into: continuation) }
            }
        } catch {
            logger.error("[pocket] the worker stream for \(key.cardId.value) ended: \(error)")
        }
    }

    /// A render that fails is opened again on the same worker. Ending here
    /// instead would leave the card static for the worker's whole life: a new
    /// render is only opened when a different execution is published, and a
    /// stop publishes none.
    func reopening(
        _ execution: TrUAPIProductExecutionProtocol,
        for key: PocketCardKey,
        into continuation: AsyncThrowingStream<RendererNode, Error>.Continuation
    ) async {
        var attempt = 0
        while !Task.isCancelled {
            do {
                try await drain(execution, for: key, into: continuation)
                attempt = 0
            } catch is CancellationError {
                return
            } catch {
                logger.warning("[pocket] the face stream for \(key.cardId.value) ended, reopening: \(error)")
            }

            try? await Task.sleep(for: reopenDelay(after: attempt))
            attempt += 1
        }
    }

    func drain(
        _ execution: TrUAPIProductExecutionProtocol,
        for key: PocketCardKey,
        into continuation: AsyncThrowingStream<RendererNode, Error>.Continuation
    ) async throws {
        for try await face in try await connect(execution, for: key) {
            try Task.checkCancellation()
            continuation.yield(face)
        }
    }

    /// The worker registers its handler after it connects, so the first renders
    /// are refused rather than answered. They are asked again at a fixed short
    /// interval instead of backing off, because the wait is a boot, not a fault.
    func connect(
        _ execution: TrUAPIProductExecutionProtocol,
        for key: PocketCardKey
    ) async throws -> AsyncThrowingStream<RendererNode, Error> {
        let request = ProductRendererRenderRequest(context: .pocketCard(cardId: key.cardId.value), payload: Data())

        return try await execution.renderWhenConnected(
            request,
            until: .now + Retry.connectWindow,
            retryEvery: Retry.connectDelay
        )
    }

    func reopenDelay(after attempt: Int) -> Duration {
        let doublings = min(attempt, Retry.maxBackoffDoublings)

        return min(Retry.reopenDelay * (1 << doublings), Retry.maxReopenDelay)
    }
}
