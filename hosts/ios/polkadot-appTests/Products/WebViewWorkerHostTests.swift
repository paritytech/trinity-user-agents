import Foundation
import Products
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
@testable import polkadot_app

private let product = try! ProductId("game.paseo")

struct WebViewWorkerHostTests {
    @Test
    func stoppingAnEarlierBootDisposesOnlyItsWebView() async throws {
        let slow = WebViewFixture()
        let replacement = WebViewFixture()
        let entered = AsyncStream<Void>.makeStream()
        let gate = AsyncStream<Void>.makeStream()
        let first = Attempt(1)
        let second = Attempt(2)
        let host = WebViewWorkerHost { _, attempt in
            if attempt.id() == first.id() {
                entered.continuation.finish()
                // Waiting on another task models a fetch that ignores its caller's cancellation.
                let fetch = Task { for await _ in gate.stream {} }
                await fetch.value
                return slow.runtime()
            }
            return replacement.runtime()
        }

        host.start(productId: product, attempt: first)
        try await withTimeout(.hangGuard) { for await _ in entered.stream {} }
        host.stop(productId: product, attemptId: first.id())
        host.start(productId: product, attempt: second)
        #expect(try await second.nextEvent() == .started)
        gate.continuation.finish()
        try await withTimeout(.hangGuard) { await slow.execution.awaitClose() }

        first.events.continuation.finish()
        #expect(try await first.nextEvent() == nil)
        #expect(slow.engine.evaluatedScripts.isEmpty)
        #expect(replacement.execution.closeCallCount == 0)
        host.stop(productId: product, attemptId: second.id())
        try await withTimeout(.hangGuard) { await replacement.execution.awaitClose() }
        #expect(replacement.engine.destroyCallCount == 1)
    }

    @Test
    func aLostWebProcessReportsItsOwnAttempt() async throws {
        let view = WebViewFixture()
        let attempt = Attempt(3)
        let host = WebViewWorkerHost { _, _ in view.runtime() }
        host.start(productId: product, attempt: attempt)
        #expect(try await attempt.nextEvent() == .started)

        view.engine.terminate()
        #expect(try await attempt.nextEvent() == .failed("WebView process terminated"))
        host.stop(productId: product, attemptId: attempt.id())
        try await withTimeout(.hangGuard) { await view.execution.awaitClose() }
    }

    @Test
    func reportsAModuleLoadErrorBeforeAcknowledgingStartup() async throws {
        let view = WebViewFixture()
        view.engine.moduleLoadFails = true
        let attempt = Attempt(1)
        let host = WebViewWorkerHost { _, _ in view.runtime() }
        host.start(productId: product, attempt: attempt)

        #expect(try await attempt.nextEvent() == .failed(String(describing: JSEngineError.moduleLoadFailed)))
        host.stop(productId: product, attemptId: attempt.id())
        try await withTimeout(.hangGuard) { await view.execution.awaitClose() }
    }

    @Test
    func reportsABuildFailureSoTheCoreCanRetryWhileClaimed() async throws {
        struct MissingWorker: Error {}

        let attempt = Attempt(4)
        let host = WebViewWorkerHost { _, _ in throw MissingWorker() }
        host.start(productId: product, attempt: attempt)
        #expect(try await attempt.nextEvent() == .failed(String(describing: MissingWorker())))
        host.stop(productId: product, attemptId: attempt.id())
    }

    /// A cancelled startup is one the core stopped itself. Nothing failed, so a failure report
    /// would blame the worker for the core's own stop.
    @Test
    func aStartupTheCoreStoppedReportsNoFailure() async throws {
        let building = AsyncStream<Void>.makeStream()
        let fetch = AsyncStream<Void>.makeStream()
        let host = WebViewWorkerHost { _, _ in
            building.continuation.finish()
            // Waiting on the stream itself models a fetch that ends when its caller is cancelled.
            for await _ in fetch.stream {}
            throw CancellationError()
        }
        let events: AsyncStream<Attempt.Event>
        do {
            let attempt = Attempt(5)
            events = attempt.events.stream
            host.start(productId: product, attempt: attempt)
            try await withTimeout(.hangGuard) { for await _ in building.stream {} }
            host.stop(productId: product, attemptId: attempt.id())
        }

        // The startup task holds the last reference to the attempt, so its events end when it does.
        let reported = try await withTimeout(.hangGuard) { await events.first { _ in true } }
        #expect(reported == nil)
    }
}

private final class Attempt: NativeWorkerAttempt, @unchecked Sendable {
    enum Event: Equatable, Sendable {
        case started
        case failed(String)
    }

    let events = AsyncStream<Event>.makeStream()
    private let value: UInt64

    init(_ value: UInt64) {
        self.value = value
        super.init(noHandle: .init())
    }

    deinit { events.continuation.finish() }

    required init(unsafeFromHandle _: UInt64) {
        fatalError("Only a simulator test can create this attempt")
    }

    override func id() -> UInt64 { value }
    override func moduleStarted() -> Bool {
        events.continuation.yield(.started)
        return true
    }

    override func failed(reason: String) { events.continuation.yield(.failed(reason)) }

    func nextEvent() async throws -> Event? {
        try await withTimeout(.hangGuard) { [events] in
            var iterator = events.stream.makeAsyncIterator()
            return await iterator.next()
        }
    }
}

private struct WebViewFixture: Sendable {
    let execution = MockProductExecution()
    let engine = MockJSEngine()

    func runtime() -> TrUAPIWorkerRuntime {
        TrUAPIWorkerRuntime(
            productUrl: URL(string: "https://product.invalid/worker.js")!,
            executionModel: .init(
                execution: execution,
                chainConnections: MockChainConnections(),
                osPermissionAsker: OSPermissionAsker()
            ),
            engineFactory: { engine }
        )
    }
}
