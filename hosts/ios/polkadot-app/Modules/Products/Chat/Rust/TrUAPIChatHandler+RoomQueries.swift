import Foundation
import TrUAPIHost

/// The lookups of the room screen (P6). Each one holds the worker while it runs.
extension TrUAPIChatHandler: ProductRoomInteracting {
    /// The `peer` of every event of the host user, until a room has more than one user (02-DESIGN §8.4).
    static let hostUserPeer = "native"

    func reactions(_ request: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem {
        try await withWorker { try await $0.chatReactionsQuery(request, timeout: ChatQueryTimeout.lookup) }
    }

    func editHistory(_ request: ProductChatEditHistoryQueryRequest) async throws -> ProductChatEditHistoryQueryItem {
        try await withWorker { try await $0.chatEditHistory(request, timeout: ChatQueryTimeout.lookup) }
    }

    /// Runs `body` with the execution of the worker, and holds the worker while it runs (02-DESIGN D10). A worker
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
