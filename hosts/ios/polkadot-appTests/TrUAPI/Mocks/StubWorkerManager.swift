import Foundation
import os
import AsyncExtensions
import Products
import StructuredConcurrency
import TrUAPIHost
@testable import polkadot_app

/// Publishes executions the way the manager does: one value carrying every
/// product, re-sent whenever any of them moves.
final class StubWorkerManager: TrUAPIWorkerManaging, @unchecked Sendable {
    private let subject = AsyncCurrentValueSubject<[ProductId: TrUAPIProductExecutionProtocol]>([:])
    private let openContexts = OSAllocatedUnfairLock<[ProductId: ProductWorkerContext]>(initialState: [:])
    private let startupWindow: Duration
    private let entered = AsyncStream<Void>.makeStream()
    private let gate = AsyncStream<Void>.makeStream()

    /// What the handlers asked for, so a test can see a request that was taken
    /// and never given back.
    let references: StubWorkerReferences

    private(set) var demands: [(productId: ProductId, transition: WorkerTransition)] = []

    init(references: StubWorkerReferences = StubWorkerReferences(), startupWindow: Duration = .seconds(1)) {
        self.references = references
        self.startupWindow = startupWindow
    }

    /// Held open so a test can dispose a handler while its ask is still in
    /// flight, which is the window the real manager has between the handler
    /// asking and the core registering the request.
    var holdsTheAsk = false

    var asks: AsyncStream<Void> { entered.stream }

    func openTheAsk() {
        gate.continuation.finish()
    }

    func ensureWorker(for productId: ProductId) async throws -> TrUAPIProductExecutionProtocol {
        entered.continuation.yield(())
        if holdsTheAsk {
            for await _ in gate.stream {}
        }
        references.acquireWorker(productId: productId)

        return try await withTimeout(startupWindow) { [self] in
            for try await execution in executions(of: productId) {
                if let execution { return execution }
            }

            throw TrUAPIWorkerError.noWorker(productId)
        }
    }

    func releaseWorker(for productId: ProductId) {
        references.releaseWorker(productId: productId)
    }

    func publish(_ executions: [ProductId: TrUAPIProductExecutionProtocol]) {
        subject.send(executions)
    }

    func demandChanged(productId: ProductId, transition: WorkerTransition) {
        demands.append((productId, transition))
    }

    func context(of productId: ProductId) -> ProductWorkerContext {
        openContexts.withLock { open in
            if let context = open[productId] { return context }

            let context = ProductWorkerContext()
            open[productId] = context
            return context
        }
    }

    func executions(of productId: ProductId) -> AnyAsyncSequence<TrUAPIProductExecutionProtocol?> {
        subject
            .map { $0[productId] }
            .eraseToAnyAsyncSequence()
    }

    func currentExecution(of productId: ProductId) -> TrUAPIProductExecutionProtocol? {
        subject.value[productId]
    }

    func shutdown() async {}
}

/// The requests a handler makes, recorded rather than counted by the core.
///
/// Handlers ask and give back from their own tasks while the test reads these
/// from its own, so the records are locked. Appending to a bare array from two
/// threads is a data race, and the test that reads it fails at random.
final class StubWorkerReferences: TrUAPIWorkerReferencing, @unchecked Sendable {
    private let records = OSAllocatedUnfairLock(
        initialState: (acquired: [ProductId](), released: [ProductId](), log: [String]())
    )

    var acquired: [ProductId] { records.withLock { $0.acquired } }
    var released: [ProductId] { records.withLock { $0.released } }
    /// Both in the order they happened. A release reaching the core before its
    /// acquire drops the count of a worker another holder is drawing from.
    var log: [String] { records.withLock { $0.log } }

    func acquireWorker(productId: ProductId) {
        records.withLock {
            $0.acquired.append(productId)
            $0.log.append("acquire \(productId)")
        }
    }

    func releaseWorker(productId: ProductId) {
        records.withLock {
            $0.released.append(productId)
            $0.log.append("release \(productId)")
        }
    }
}
