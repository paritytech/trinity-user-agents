import Foundation
import TrUAPIHost

/// The queries of the room screen. Each one holds the worker while it runs.
extension TrUAPIChatHandler {
    /// Runs `body` with the execution of the worker, and holds the worker while it runs. A worker
    /// that does not run starts first.
    func withWorker<Answer: Sendable>(
        _ body: @Sendable (TrUAPIProductExecutionProtocol) async throws -> Answer
    ) async throws -> Answer {
        guard !disposed else { throw CancellationError() }

        let execution: TrUAPIProductExecutionProtocol
        do {
            execution = try await workers.ensureWorker(for: productId)
        } catch {
            workers.releaseWorker(for: productId)
            throw error
        }
        defer { workers.releaseWorker(for: productId) }

        return try await body(execution)
    }
}
