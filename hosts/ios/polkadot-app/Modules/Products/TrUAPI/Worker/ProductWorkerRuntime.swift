import Foundation
import Products
import TrUAPIHost

typealias ProductWorkerRuntimeProvider = @Sendable () throws -> any ProductWorkerRuntime

protocol ProductWorkerRuntime: Sendable {
    func claimWorker(productId: ProductId) -> NativeWorkerClaim
    func workerExecutionState(productId: ProductId) -> AsyncStream<NativeWorkerExecutionState>
}

extension TrUAPIHostRuntime: ProductWorkerRuntime {}

extension NativeWorkerExecutionState {
    /// The execution to call while the worker runs.
    var execution: NativeWorkerExecution? {
        if case let .running(execution) = self { execution } else { nil }
    }
}

extension ProductWorkerRuntime {
    /// The execution of the product's worker if it runs now.
    func runningExecution(_ productId: ProductId) async -> NativeWorkerExecution? {
        var states = workerExecutionState(productId: productId).makeAsyncIterator()
        return await states.next()?.execution
    }

    /// The execution of the product's worker once it runs, however long that
    /// takes. `nil` if the core stops reporting the worker, which is at session
    /// end, or the caller is cancelled first.
    func executionWhenRunning(_ productId: ProductId) async -> NativeWorkerExecution? {
        for await worker in workerExecutionState(productId: productId) {
            if let execution = worker.execution { return execution }
        }
        return nil
    }

    /// Runs `body` on each execution the product's worker publishes, and cancels
    /// it when the worker stops running. Returns once the core stops reporting
    /// the worker, which is at session end, or when the caller is cancelled.
    func withRunningWorker(
        _ productId: ProductId,
        _ body: @escaping @Sendable (NativeWorkerExecution) async -> Void
    ) async {
        var running: Task<Void, Never>?

        for await worker in workerExecutionState(productId: productId) {
            guard !Task.isCancelled else { break }
            guard let execution = worker.execution else {
                running?.cancel()
                await running?.value
                running = nil
                continue
            }
            if running == nil {
                running = Task { await body(execution) }
            }
        }

        running?.cancel()
        await running?.value
    }

    /// Draws `request` on the product's worker for as long as the caller listens.
    ///
    /// The stream is silent while the worker is not running. It opens on each
    /// execution the worker publishes, and again on the same one when a render
    /// ends or fails, so a worker that restarts or reconnects keeps drawing.
    func renderFollowingWorker<Output: Sendable>(
        _ productId: ProductId,
        _ request: ProductRendererRenderRequest,
        logger: LoggerProtocol,
        as output: @escaping @Sendable (RendererNode) -> Output
    ) -> AsyncThrowingStream<Output, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                await withRunningWorker(productId) { execution in
                    await reopening(execution, request, logger: logger) { continuation.yield(output($0)) }
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}

/// A render that ends or fails is opened again on the same execution. Ending
/// here instead would leave what it drew static for the worker's whole life: a
/// new render is only opened when the worker is published again, and a render
/// that stops publishes nothing.
private func reopening(
    _ execution: NativeWorkerExecution,
    _ request: ProductRendererRenderRequest,
    logger: LoggerProtocol,
    yield: @Sendable (RendererNode) -> Void
) async {
    var attempt = 0
    while !Task.isCancelled {
        do {
            for try await node in try execution.render(request) {
                try Task.checkCancellation()
                yield(node)
            }
            attempt = 0
        } catch is CancellationError {
            return
        } catch {
            logger.warning("[worker] the render of \(request.context) ended, reopening: \(error)")
        }

        try? await Task.sleep(for: reopenDelay(after: attempt))
        attempt += 1
    }
}

/// How long to wait before opening a stream from the core again. Doubling from
/// a second up to half a minute: a stream that is simply slow is picked up at
/// once, and a broken one is not asked on a loop.
func reopenDelay(after attempt: Int) -> Duration {
    let doublings = min(attempt, 5)

    return min(.seconds(1) * (1 << doublings), .seconds(30))
}
