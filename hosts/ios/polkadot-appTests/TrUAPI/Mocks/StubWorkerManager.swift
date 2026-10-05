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

    /// What the handlers asked for, so a test can see a request that was taken
    /// and never given back.
    let references: StubWorkerReferences

    private(set) var demands: [(productId: ProductId, transition: WorkerTransition)] = []

    init(references: StubWorkerReferences = StubWorkerReferences(), startupWindow: Duration = .seconds(1)) {
        self.references = references
        self.startupWindow = startupWindow
    }

    func ensureWorker(for productId: ProductId) async throws -> TrUAPIProductExecutionProtocol {
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

/// The references a modality holder takes, recorded rather than counted by the
/// core.
final class StubWorkerReferences: TrUAPIWorkerReferencing, @unchecked Sendable {
    private(set) var acquired: [ProductId] = []
    private(set) var released: [ProductId] = []

    func acquireWorker(productId: ProductId) {
        acquired.append(productId)
    }

    func releaseWorker(productId: ProductId) {
        released.append(productId)
    }
}
