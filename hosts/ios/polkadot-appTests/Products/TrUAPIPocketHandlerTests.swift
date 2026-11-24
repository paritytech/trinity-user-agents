import Products
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
@testable import polkadot_app

private let gameProductId = try! ProductId("game.paseo")

/// The render stream is what makes a card live. Both ways it goes wrong are
/// invisible on screen: a stream nothing reopens, and a stream torn down for a
/// worker that never moved.
struct TrUAPIPocketHandlerTests {
    @Test(.timeLimit(.minutes(1)))
    func keepsDrawingWhenTheCoreReportsTheRunningWorkerAgain() async throws {
        let workers = StubWorkerRuntime()
        let execution = MockProductExecution()
        execution.keepsRenderStreamOpen = true
        let streams = makeStreams(workers: workers)
        let drawing = Task {
            for try await face in streams.renderFaces(for: loyalty) {
                if case let .string(text) = face { return text == "still live" }
            }
            return false
        }
        defer { drawing.cancel() }

        var delivered = workers.executionsDelivered.makeAsyncIterator()
        _ = await delivered.next()
        workers.publish([gameProductId: execution])
        try await withTimeout(.hangGuard) { await execution.awaitRenderRequest() }
        workers.publish([gameProductId: execution])
        workers.publish([gameProductId: execution])
        for _ in 0 ..< 3 {
            _ = await delivered.next()
        }

        let original = try #require(execution.renderObservers.first)
        original.onUpdate(node: .string(text: "still live"))
        #expect(try await withTimeout(.hangGuard) { try await drawing.value })
        #expect(execution.renderRequests.count == 1)
    }

    /// A worker that restarts is a new execution, and the card has to draw from
    /// it rather than stay on the stream of the one that went away.
    @Test
    func opensARenderOnTheExecutionThatReplacesTheWorker() async throws {
        let workers = StubWorkerRuntime()
        let first = MockProductExecution()
        first.keepsRenderStreamOpen = true
        let streams = makeStreams(workers: workers)

        workers.publish([gameProductId: first])
        try await streams.start()
        defer { streams.dispose() }

        let drawing = Task { for try await _ in streams.renderFaces(for: loyalty) {} }
        defer { drawing.cancel() }

        try await withTimeout(.hangGuard) { await first.awaitRenderRequest() }

        let replacement = MockProductExecution()
        replacement.keepsRenderStreamOpen = true
        workers.publish([:])
        workers.publish([gameProductId: replacement])
        try await withTimeout(.hangGuard) { await replacement.awaitRenderRequest() }

        #expect(first.renderRequests.count == 1)
        #expect(workers.references.log == ["acquire \(gameProductId)"])
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardKey(productId: gameProductId, cardId: PocketCardId(value: "loyalty"))

private func makeStreams(workers: StubWorkerRuntime) -> TrUAPIPocketHandler {
    TrUAPIPocketHandler(productId: gameProductId, runtime: { workers })
}
