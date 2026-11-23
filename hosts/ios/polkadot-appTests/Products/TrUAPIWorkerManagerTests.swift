import Foundation
import Products
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
@testable import polkadot_app

private let product = try! ProductId("game.paseo")

struct TrUAPIWorkerManagerTests {
    @Test
    func stoppingAnEarlierBootDisposesOnlyItsWebView() async throws {
        let slow = WebViewFixture()
        let replacement = WebViewFixture()
        let entered = AsyncStream<Void>.makeStream()
        let gate = AsyncStream<Void>.makeStream()
        let first = Attempt(1)
        let second = Attempt(2)
        let manager = makeManager { _, _, attempt in
            if attempt.id() == first.id() {
                entered.continuation.finish()
                // Waiting on another task models a fetch that ignores its caller's cancellation.
                let fetch = Task { for await _ in gate.stream {} }
                await fetch.value
                return slow.runtime()
            }
            return replacement.runtime()
        }

        manager.start(productId: product, attempt: first)
        try await withTimeout(.hangGuard) { for await _ in entered.stream {} }
        manager.stop(productId: product, attemptId: first.id())
        manager.start(productId: product, attempt: second)
        try await withTimeout(.hangGuard) { for await _ in replacement.loaded.stream {} }
        gate.continuation.finish()
        try await withTimeout(.hangGuard) { await slow.execution.awaitClose() }

        #expect(slow.engine.evaluatedScripts.isEmpty)
        #expect(replacement.execution.closeCallCount == 0)
        manager.stop(productId: product, attemptId: second.id())
        try await withTimeout(.hangGuard) { await replacement.execution.awaitClose() }
        #expect(replacement.engine.destroyCallCount == 1)
    }

    @Test
    func aLostWebProcessReportsItsOwnAttempt() async throws {
        let view = WebViewFixture()
        let attempt = Attempt(3)
        let manager = makeManager { _, _, _ in view.runtime() }
        manager.start(productId: product, attempt: attempt)
        try await withTimeout(.hangGuard) { for await _ in view.loaded.stream {} }

        view.engine.terminate()
        let reason = try await withTimeout(.hangGuard) {
            var failures = attempt.failures.stream.makeAsyncIterator()
            return await failures.next()
        }
        #expect(reason == "WebView process terminated")
        manager.stop(productId: product, attemptId: attempt.id())
        try await withTimeout(.hangGuard) { await view.execution.awaitClose() }
    }

    @Test
    func reportsABuildFailureSoTheCoreCanRetryWhileClaimed() async throws {
        struct MissingWorker: Error {}

        let attempt = Attempt(4)
        let manager = makeManager { _, _, _ in throw MissingWorker() }
        manager.start(productId: product, attempt: attempt)
        let reason = try await withTimeout(.hangGuard) {
            var failures = attempt.failures.stream.makeAsyncIterator()
            return await failures.next()
        }
        #expect(reason == String(describing: MissingWorker()))
        manager.stop(productId: product, attemptId: attempt.id())
    }

    private func makeManager(build: @escaping TrUAPIWorkerManager.Builder) -> TrUAPIWorkerManager {
        TrUAPIWorkerManager(build: build, runtime: { throw CancellationError() }, presentationFallback: { nil })
    }
}

private final class Attempt: NativeWorkerAttempt, @unchecked Sendable {
    let failures = AsyncStream<String>.makeStream()
    private let value: UInt64

    init(_ value: UInt64) {
        self.value = value
        super.init(noHandle: .init())
    }

    required init(unsafeFromHandle _: UInt64) {
        fatalError("Only a simulator test can create this attempt")
    }

    override func id() -> UInt64 { value }
    override func failed(reason: String) { failures.continuation.yield(reason) }
}

private struct WebViewFixture: Sendable {
    let execution = MockProductExecution()
    let engine = MockJSEngine()
    let loaded = AsyncStream<Void>.makeStream()

    func runtime() -> TrUAPIWorkerRuntime {
        TrUAPIWorkerRuntime(
            productUrl: URL(string: "https://product.invalid/worker.js")!,
            executionModel: .init(
                execution: execution,
                chainConnections: MockChainConnections(),
                osPermissionAsker: OSPermissionAsker(),
                acknowledgeStartup: { loaded.continuation.finish(); return true }
            ),
            engineFactory: { engine }
        )
    }
}
