import Foundation
import os
import AsyncExtensions
import Products
import TrUAPIHost
@testable import polkadot_app

final class StubWorkerRuntime: ProductWorkerRuntime, @unchecked Sendable {
    private let states = AsyncCurrentValueSubject<[ProductId: NativeWorkerExecutionState]>([:])
    private let entered = AsyncStream<Void>.makeStream()
    private let delivered = AsyncStream<Void>.makeStream()
    private let lookups = AsyncStream<Void>.makeStream()

    let references = StubWorkerReferences()

    var readinessChecks: AsyncStream<Void> { entered.stream }
    var executionsDelivered: AsyncStream<Void> { delivered.stream }
    var executionLookups: AsyncStream<Void> { lookups.stream }

    func claimWorker(productId: ProductId) -> NativeWorkerClaim {
        references.claim(productId)
    }

    func publish(_ executions: [ProductId: MockProductExecution]) {
        states.send(executions.mapValues { .running(execution: $0) })
    }

    func fail(_ productId: ProductId, reason: String) {
        states.send([productId: .failed(reason: reason)])
    }

    func workerExecutionState(productId: ProductId) -> AsyncStream<NativeWorkerExecutionState> {
        entered.continuation.yield(())
        let iterator = StateIterator(states.makeAsyncIterator())
        return AsyncStream(unfolding: { [delivered] in
            guard let snapshot = await iterator.next() else { return nil }
            delivered.continuation.yield(())
            return snapshot[productId] ?? .pending
        })
    }

    func currentWorkerExecution(productId: ProductId) -> NativeWorkerExecution? {
        lookups.continuation.yield(())
        guard case let .running(execution)? = states.value[productId] else { return nil }
        return execution
    }
}

/// Each observer advances its iterator in the same task that consumes the returned stream.
private final class StateIterator<Iterator: AsyncIteratorProtocol>: @unchecked Sendable {
    private var iterator: Iterator

    init(_ iterator: Iterator) { self.iterator = iterator }

    func next() async -> Iterator.Element? { try? await iterator.next() }
}

/// The requests a handler makes, recorded rather than counted by the core.
///
/// Handlers ask and give back from their own tasks while the test reads these
/// from its own, so the records are locked. Appending to a bare array from two
/// threads is a data race, and the test that reads it fails at random.
final class StubWorkerReferences: @unchecked Sendable {
    private let records = OSAllocatedUnfairLock(
        initialState: (acquired: [ProductId](), released: [ProductId](), log: [String](), releaseCounts: [Int]())
    )

    var releaseCounts: [Int] { records.withLock { $0.releaseCounts } }
    var acquired: [ProductId] { records.withLock { $0.acquired } }
    var released: [ProductId] { records.withLock { $0.released } }
    /// Both in the order they happened. A release reaching the core before its
    /// acquire drops the count of a worker another holder is drawing from.
    var log: [String] { records.withLock { $0.log } }

    func claim(_ productId: ProductId) -> NativeWorkerClaim {
        let index = records.withLock {
            $0.acquired.append(productId)
            $0.log.append("acquire \(productId)")
            $0.releaseCounts.append(0)
            return $0.releaseCounts.count - 1
        }
        return Claim { [self] in
            records.withLock {
                $0.releaseCounts[index] += 1
                $0.released.append(productId)
                $0.log.append("release \(productId)")
            }
        }
    }

    private final class Claim: NativeWorkerClaim, @unchecked Sendable {
        let onRelease: @Sendable () -> Void

        init(_ onRelease: @escaping @Sendable () -> Void) {
            self.onRelease = onRelease
            super.init(noHandle: .init())
        }

        required init(unsafeFromHandle _: UInt64) { fatalError("Test claim") }

        override func release() { onRelease() }
    }
}
