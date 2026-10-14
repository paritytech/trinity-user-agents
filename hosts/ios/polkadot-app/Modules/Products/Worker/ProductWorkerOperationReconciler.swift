import Foundation
import os
import Products

/// Keeps a worker alive for every persisted operation. Subscribes to the
/// operation store and holds exactly one worker token per open operation, so a
/// worker starts whenever an operation exists (including ones restored from a
/// previous run) and is released when the last one ends.
final class ProductWorkerOperationReconciler: @unchecked Sendable {
    private struct Key: Hashable {
        let productId: ProductId
        let id: UInt32
    }

    private struct State {
        var tokens: [Key: ProductWorkerToken] = [:]
        var subscription: Task<Void, Never>?
    }

    private let store: ProductOperationStoring
    private let manager: ProductWorkerManaging
    private let logger: LoggerProtocol
    private let state = OSAllocatedUnfairLock<State>(initialState: State())

    init(
        store: ProductOperationStoring,
        manager: ProductWorkerManaging,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.store = store
        self.manager = manager
        self.logger = logger
    }

    /// Starts reconciling. Call once at launch.
    func start() {
        state.withLock { state in
            guard state.subscription == nil else { return }
            state.subscription = Task { [weak self] in
                await self?.run()
            }
        }
    }
}

private extension ProductWorkerOperationReconciler {
    func run() async {
        do {
            logger.debug("Starting operations reconciler")

            for try await records in store.subscribe() {
                await reconcile(records)
            }
        } catch {
            logger.error("Worker operation reconciliation stopped: \(error)")
        }
    }

    func reconcile(_ records: [ProductOperationRecord]) async {
        let desired = Set(records.map { Key(productId: $0.productId, id: $0.id) })

        let removedTokens = state.withLock { state -> [ProductWorkerToken] in
            let staleKeys = state.tokens.keys.filter { !desired.contains($0) }
            return staleKeys.compactMap { state.tokens.removeValue(forKey: $0) }
        }
        removedTokens.forEach { $0.unlock() }

        let missing = state.withLock { state in
            desired.filter { state.tokens[$0] == nil }
        }

        for key in missing {
            let lease = await manager.acquire(productId: key.productId)

            let kept = state.withLock { state -> Bool in
                guard state.tokens[key] == nil else { return false }
                state.tokens[key] = lease.token
                return true
            }

            if !kept {
                lease.token.unlock()
            }
        }
    }
}
