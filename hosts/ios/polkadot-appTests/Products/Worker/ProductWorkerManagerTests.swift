import Testing
import Foundation
import os
import Products
@testable import polkadot_app

private final class FakeWorker: ProductWorkerRunning, @unchecked Sendable {
    private let onDispose: @Sendable () -> Void
    init(onDispose: @escaping @Sendable () -> Void) { self.onDispose = onDispose }
    func dispose() async { onDispose() }
}

private final class FakeWorkerFactory: ProductWorkerFactory, @unchecked Sendable {
    private let counts = OSAllocatedUnfairLock(initialState: (started: 0, disposed: 0))
    private let disposals: AsyncStream<Int>
    private let disposalContinuation: AsyncStream<Int>.Continuation

    init() {
        (disposals, disposalContinuation) = AsyncStream<Int>.makeStream(bufferingPolicy: .unbounded)
    }

    func startWorker(productId _: ProductId) async throws -> ProductWorkerRunning {
        counts.withLock { $0.started += 1 }
        return FakeWorker { [counts, disposalContinuation] in
            let total = counts.withLock { state -> Int in
                state.disposed += 1
                return state.disposed
            }
            disposalContinuation.yield(total)
        }
    }

    var started: Int { counts.withLock { $0.started } }
    var disposed: Int { counts.withLock { $0.disposed } }

    /// Awaits the given number of disposals so tests observe teardown as an event
    /// rather than polling.
    func awaitDisposed(_ target: Int) async {
        guard disposed < target else { return }
        for await total in disposals where total >= target {
            return
        }
    }
}

@Suite("ProductWorkerManager")
struct ProductWorkerManagerTests {
    @Test("first acquire starts the worker, extra consumers do not restart it")
    func startsOnceForConcurrentConsumers() async {
        let factory = FakeWorkerFactory()
        let manager = ProductWorkerManager(factory: factory)

        let chat = await manager.acquire(productId: "getcash")
        let spa = await manager.acquire(productId: "getcash")

        #expect(factory.started == 1)
        #expect(factory.disposed == 0)

        withExtendedLifetime((chat, spa)) {}
    }

    @Test("worker is disposed only after the last consumer releases")
    func disposesOnLastRelease() async {
        let factory = FakeWorkerFactory()
        let manager = ProductWorkerManager(factory: factory)

        let chat = await manager.acquire(productId: "getcash")
        let spa = await manager.acquire(productId: "getcash")

        chat.token.unlock()

        // One consumer left: re-acquiring reuses the worker rather than restarting.
        let again = await manager.acquire(productId: "getcash")
        #expect(factory.started == 1)
        #expect(factory.disposed == 0)

        again.token.unlock()
        spa.token.unlock()

        await factory.awaitDisposed(1)
        #expect(factory.started == 1)
        #expect(factory.disposed == 1)
    }

    @Test("token unlock is idempotent")
    func tokenUnlockIsIdempotent() async {
        let factory = FakeWorkerFactory()
        let manager = ProductWorkerManager(factory: factory)

        let a = await manager.acquire(productId: "getcash")
        let b = await manager.acquire(productId: "getcash")

        a.token.unlock()
        a.token.unlock() // second release must not drop b's claim

        let c = await manager.acquire(productId: "getcash")
        #expect(factory.started == 1) // worker survived, no restart
        #expect(factory.disposed == 0)

        withExtendedLifetime((b, c)) {}
    }

    @Test("dropping a lease releases its claim")
    func deinitReleasesClaim() async {
        let factory = FakeWorkerFactory()
        let manager = ProductWorkerManager(factory: factory)

        do {
            _ = await manager.acquire(productId: "getcash")
        }

        await factory.awaitDisposed(1)
        #expect(factory.disposed == 1)
    }

    @Test("rapid acquire/release never leaks a worker")
    func churnStaysBalanced() async {
        let factory = FakeWorkerFactory()
        let manager = ProductWorkerManager(factory: factory)

        for _ in 0 ..< 20 {
            let lease = await manager.acquire(productId: "getcash")
            lease.token.unlock()
        }

        await factory.awaitDisposed(20)
        #expect(factory.started == 20)
        #expect(factory.disposed == 20)
    }
}
