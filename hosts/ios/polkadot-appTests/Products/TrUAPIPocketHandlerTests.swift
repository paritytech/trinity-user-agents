import Products
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The render stream is what makes a card live. Both ways it goes wrong are
/// invisible on screen: a stream nothing reopens, and a stream torn down for a
/// worker that never moved.
struct TrUAPIPocketHandlerTests {
    /// The manager publishes every product's execution in one value, so it
    /// re-sends whenever any worker starts or stops. Reopening on those would
    /// drop a live render and pay the connect retries again, for a worker this
    /// card's product never moved.
    @Test
    func doesNotReopenTheRenderWhenAnotherProductsWorkerMoves() async throws {
        let workers = StubWorkerManager()
        let execution = MockProductExecution()
        execution.keepsRenderStreamOpen = true
        let streams = makeStreams(workers: workers)

        let drawing = Task {
            for try await _ in streams.renderFaces(for: loyalty) {
                return true
            }
            return false
        }
        defer { drawing.cancel() }

        // Subscribed before anything is published, so the count of deliveries
        // below starts from the empty map the handler opens with.
        try await withTimeout(.hangGuard) { await workers.awaitExecutionsDelivered() }
        workers.publish(["game.paseo": execution])
        try await withTimeout(.hangGuard) { await execution.awaitRenderRequest() }
        workers.publish(["game.paseo": execution, "shop.paseo": MockProductExecution()])
        workers.publish(["game.paseo": execution])
        // The last republish arriving means the one before it has been handled.
        for _ in 0 ..< 3 {
            try await withTimeout(.hangGuard) { await workers.awaitExecutionsDelivered() }
        }

        // A reopen cancels the drain of the first stream, so this face would never reach the card.
        let live = try #require(execution.renderContinuations.first)
        live.yield(.string(text: "still live"))

        #expect(try await withTimeout(.hangGuard) { try await drawing.value })
        #expect(execution.renderRequests.count == 1)
    }

    /// A worker that restarts is a new execution, and the card has to draw from
    /// it rather than stay on the stream of the one that went away.
    @Test
    func opensARenderOnTheExecutionThatReplacesTheWorker() async throws {
        let workers = StubWorkerManager()
        let first = MockProductExecution()
        first.keepsRenderStreamOpen = true
        let streams = makeStreams(workers: workers)

        let drawing = Task { for try await _ in streams.renderFaces(for: loyalty) {} }
        defer { drawing.cancel() }

        workers.publish(["game.paseo": first])
        try await withTimeout(.hangGuard) { await first.awaitRenderRequest() }

        let replacement = MockProductExecution()
        replacement.keepsRenderStreamOpen = true
        workers.publish(["game.paseo": replacement])
        try await withTimeout(.hangGuard) { await replacement.awaitRenderRequest() }

        #expect(first.renderRequests.count == 1)
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

private func makeStreams(workers: StubWorkerManager) -> TrUAPIPocketHandler {
    TrUAPIPocketHandler(productId: "game.paseo", workers: workers)
}
