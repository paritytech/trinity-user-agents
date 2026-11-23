import Foundation
import os
import Products
import TrUAPIHost

/// The live side of a face: the product's render stream, and the actions going
/// back to it.
protocol PocketFaceStreaming: Sendable {
    /// Each tree replaces the previous face for this card.
    func renderFaces(for key: PocketCardKey) -> AsyncThrowingStream<RendererNode, Error>

    func send(action: String, payload: Data, for key: PocketCardKey)
}

/// Sends Pocket calls over each ready execution and reopens interrupted renders.
final class TrUAPIPocketHandler: PocketFaceStreaming, Sendable {
    private enum Retry {
        /// A worker that has just booted is not yet listening, so the first
        /// renders are expected to be refused.
        static let connectWindow = Duration.seconds(10)
        static let connectDelay = Duration.milliseconds(250)
    }

    private let workers: any TrUAPIWorkerManaging
    private let logger: LoggerProtocol

    init(workers: any TrUAPIWorkerManaging, logger: LoggerProtocol = Logger.shared) {
        self.workers = workers
        self.logger = logger
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

            try? await Task.sleep(for: pocketReopenDelay(after: attempt))
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
}

/// How long to wait before opening a failed Pocket stream again. Doubling from
/// a second up to half a minute: a stream that is simply slow is picked up at
/// once, and a broken one is not asked on a loop.
func pocketReopenDelay(after attempt: Int) -> Duration {
    let doublings = min(attempt, 5)

    return min(.seconds(1) * (1 << doublings), .seconds(30))
}
