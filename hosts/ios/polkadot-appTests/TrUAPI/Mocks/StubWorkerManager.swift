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
    /// Long by default so a loaded runner never trips it; tests about a worker that never comes up pass a short one.
    private let startupWindow: Duration
    private let entered = AsyncStream<Void>.makeStream()
    private let gate = AsyncStream<Void>.makeStream()
    private let lookups = AsyncStream<Void>.makeStream()
    private let deliveredSignal: AsyncStream<Void>.Continuation
    private var deliveredSignals: AsyncStream<Void>.AsyncIterator

    /// What the handlers asked for, so a test can see a request that was taken
    /// and never given back.
    let references: StubWorkerReferences

    private(set) var demands: [(productId: ProductId, transition: WorkerTransition)] = []

    init(references: StubWorkerReferences = StubWorkerReferences(), startupWindow: Duration = .seconds(10)) {
        let (delivered, continuation) = AsyncStream.makeStream(of: Void.self)
        deliveredSignal = continuation
        deliveredSignals = delivered.makeAsyncIterator()
        self.references = references
        self.startupWindow = startupWindow
    }

    /// Lets a test dispose a claimed worker while its readiness check is suspended.
    var suspendsReadiness = false

    var readinessChecks: AsyncStream<Void> { entered.stream }

    /// One element per `currentExecution(of:)` call, so a test can wait until a caller has looked for a worker.
    var executionLookups: AsyncStream<Void> { lookups.stream }

    func resumeReadiness() {
        gate.continuation.finish()
    }

    func claim(_ productId: ProductId) throws -> any TrUAPIWorkerClaiming {
        references.claim(productId)
    }

    func waitUntilReady(_ productId: ProductId) async throws {
        entered.continuation.yield(())
        if suspendsReadiness {
            for await _ in gate.stream {}
        }

        try await withTimeout(startupWindow) { [self] in
            for try await execution in executions(of: productId) {
                if execution != nil { return }
            }

            throw TrUAPIWorkerError.noWorker(productId)
        }
    }

    func publish(_ executions: [ProductId: TrUAPIProductExecutionProtocol]) {
        subject.send(executions)
    }

    /// Returns once one more value has reached a follower of `executions(of:)`,
    /// counting the value a new follower starts from. A follower takes the next
    /// value only after handling the last, so an arrival proves the value before
    /// it was handled. Counts every follower, `waitUntilReady` included.
    func awaitExecutionsDelivered() async {
        _ = await deliveredSignals.next()
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
        let delivered = deliveredSignal
        return subject
            .map { $0[productId] }
            .handleEvents(onElement: { _ in delivered.yield() })
            .eraseToAnyAsyncSequence()
    }

    func currentExecution(of productId: ProductId) -> TrUAPIProductExecutionProtocol? {
        let execution = subject.value[productId]
        lookups.continuation.yield(())
        return execution
    }

    func shutdown() async {}

    func reload(productId _: ProductId) async {}
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

    private let onRelease: @Sendable (ProductId) -> Void

    init(onRelease: @escaping @Sendable (ProductId) -> Void = { _ in }) {
        self.onRelease = onRelease
    }

    var acquired: [ProductId] { records.withLock { $0.acquired } }
    var released: [ProductId] { records.withLock { $0.released } }
    /// Both in the order they happened. A release reaching the core before its
    /// acquire drops the count of a worker another holder is drawing from.
    var log: [String] { records.withLock { $0.log } }

    func claim(_ productId: ProductId) -> any TrUAPIWorkerClaiming {
        records.withLock {
            $0.acquired.append(productId)
            $0.log.append("acquire \(productId)")
        }
        return Claim { [self] in
            records.withLock {
                $0.released.append(productId)
                $0.log.append("release \(productId)")
            }
            onRelease(productId)
        }
    }

    private final class Claim: TrUAPIWorkerClaiming, Sendable {
        let onRelease: @Sendable () -> Void

        init(_ onRelease: @escaping @Sendable () -> Void) {
            self.onRelease = onRelease
        }

        func release() { onRelease() }
    }

    var exited: [ProductId] {
        records
            .withLock {
                $0.log.filter { $0.hasPrefix("exited ") }.compactMap { try? ProductId(String($0.dropFirst(7))) }
            }
    }

    func workerExited(productId: ProductId) {
        records.withLock { $0.log.append("exited \(productId)") }
    }
}
