import Testing
import Foundation
import os
import AsyncExtensions
import Products
@testable import polkadot_app

private final class NoopWorker: ProductWorkerRunning, @unchecked Sendable {
    func dispose() async {}
}

/// Records how many worker claims are currently held and lets a test await a
/// target count, so reconciliation is observed as an event rather than polled.
private final class CountingWorkerManager: ProductWorkerManaging, @unchecked Sendable {
    private let count = OSAllocatedUnfairLock(initialState: 0)
    private let changes: AsyncStream<Int>
    private let continuation: AsyncStream<Int>.Continuation

    init() {
        (changes, continuation) = AsyncStream<Int>.makeStream(bufferingPolicy: .unbounded)
    }

    func acquire(productId _: ProductId) async -> ProductWorkerLease {
        let held = count.withLock { $0 += 1; return $0 }
        continuation.yield(held)

        let token = ProductWorkerToken { [count, continuation] in
            let remaining = count.withLock { $0 -= 1; return $0 }
            continuation.yield(remaining)
        }

        return ProductWorkerLease(token: token, result: .success(NoopWorker()))
    }

    func awaitCount(_ target: Int) async {
        if (count.withLock { $0 }) == target {
            return
        }
        for await held in changes where held == target {
            return
        }
    }
}

/// A store whose snapshots the test pushes directly.
private final class ControllableOperationStore: ProductOperationStoring, @unchecked Sendable {
    private let snapshots: AsyncStream<[ProductOperationRecord]>
    private let continuation: AsyncStream<[ProductOperationRecord]>.Continuation

    init() {
        (snapshots, continuation) = AsyncStream<[ProductOperationRecord]>.makeStream(bufferingPolicy: .unbounded)
    }

    func emit(_ records: [ProductOperationRecord]) { continuation.yield(records) }

    func subscribe() -> AnyAsyncSequence<[ProductOperationRecord]> {
        snapshots.eraseToAnyAsyncSequence()
    }

    func save(_: ProductOperationRecord) async throws {}
    func delete(productId _: ProductId, id _: UInt32) async throws {}
    func all() async throws -> [ProductOperationRecord] { [] }
    func clearAll() async throws {}
}

@Suite("ProductWorkerOperationReconciler")
struct ProductWorkerOperationReconcilerTests {
    private func record(_ productId: ProductId, _ id: UInt32) -> ProductOperationRecord {
        ProductOperationRecord(productId: productId, id: id, label: nil, startedAt: Date(timeIntervalSince1970: 0))
    }

    @Test("holds one worker per operation and releases when operations disappear")
    func reconcilesWorkersToOperations() async {
        let store = ControllableOperationStore()
        let manager = CountingWorkerManager()
        let reconciler = ProductWorkerOperationReconciler(store: store, manager: manager)

        reconciler.start()

        // One operation opens -> one worker.
        store.emit([record("getcash", 1)])
        await manager.awaitCount(1)

        // A second operation for another product -> two workers.
        store.emit([record("getcash", 1), record("chatapp", 2)])
        await manager.awaitCount(2)

        // The first operation ends -> back to one worker.
        store.emit([record("chatapp", 2)])
        await manager.awaitCount(1)

        // All operations gone -> no workers.
        store.emit([])
        await manager.awaitCount(0)
    }
}
