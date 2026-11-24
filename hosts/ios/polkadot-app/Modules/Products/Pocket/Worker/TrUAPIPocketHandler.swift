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
    private enum Retry {
        /// A worker that has just booted is not yet listening, so the first
        /// renders are expected to be refused.
        static let connectWindow = Duration.seconds(10)
        static let connectDelay = Duration.milliseconds(250)
    }

    let productId: ProductId
    private let runtime: ProductWorkerRuntimeProvider
    private let logger: LoggerProtocol

    private let state = OSAllocatedUnfairLock(initialState: State())

    private struct State {
        var claim: NativeWorkerClaim?
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

    func start() async throws {
        let host = try runtime()
        try state.withLock { state in
            guard !state.disposed, state.claim == nil else { throw CancellationError() }

            state.claim = host.claimWorker(productId: productId)
        }

        do {
            try await host.waitForWorker(productId)
            guard !state.withLock({ $0.disposed }) else { throw CancellationError() }
        } catch {
            dispose()
            throw error
        }
    }

    func dispose() {
        let claim = state.withLock { state in
            state.disposed = true
            defer { state.claim = nil }
            return state.claim
        }
        claim?.release()
    }

    func renderFaces(for key: PocketCardKey) -> AsyncThrowingStream<RendererNode, Error> {
        AsyncThrowingStream { continuation in
            let task = Task { await stream(key, into: continuation) }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    func send(action: String, payload: Data, for key: PocketCardKey) {
        guard let execution = try? runtime().currentWorkerExecution(productId: key.productId) else { return }

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

    /// A restart replaces the render stream with one on the new execution.
    func followExecutions(
        of key: PocketCardKey,
        into continuation: AsyncThrowingStream<RendererNode, Error>.Continuation
    ) async {
        var opened: Task<Void, Never>?
        defer { opened?.cancel() }

        do {
            for await state in try runtime().workerExecutionState(productId: key.productId) {
                switch state {
                case let .running(execution):
                    if opened == nil {
                        opened = Task { await reopening(execution, for: key, into: continuation) }
                    }
                case .idle,
                     .pending,
                     .failed:
                    opened?.cancel()
                    opened = nil
                }
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
        _ execution: TrUAPIWorkerOperations,
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

            try? await Task.sleep(for: pocketReopenDelay(after: attempt))
            attempt += 1
        }
    }

    func drain(
        _ execution: TrUAPIWorkerOperations,
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
        _ execution: TrUAPIWorkerOperations,
        for key: PocketCardKey
    ) async throws -> AsyncThrowingStream<RendererNode, Error> {
        let request = ProductRendererRenderRequest(context: .pocketCard(cardId: key.cardId.value), payload: Data())

        return try await execution.renderWhenConnected(
            request,
            until: .now + Retry.connectWindow,
            retryEvery: Retry.connectDelay
        )
    }
}

/// How long to wait before opening a failed Pocket stream again. Doubling from
/// a second up to half a minute: a stream that is simply slow is picked up at
/// once, and a broken one is not asked on a loop.
func pocketReopenDelay(after attempt: Int) -> Duration {
    let doublings = min(attempt, 5)

    return min(.seconds(1) * (1 << doublings), .seconds(30))
}
