import Foundation
import AsyncExtensions
import Products
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

        let drawing = Task { for try await _ in streams.renderFaces(for: loyalty) {} }
        try await settle()

        workers.publish(["game.paseo": execution])
        try await settle()
        workers.publish(["game.paseo": execution, "shop.paseo": MockProductExecution()])
        try await settle()
        workers.publish(["game.paseo": execution])
        try await settle()
        drawing.cancel()

        #expect(execution.renderRequests.count == 1)
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

/// The handler hands its work to tasks, so the assertions wait for them rather
/// than for a fixed time.
private func settle() async throws {
    for _ in 0 ..< 20 {
        await Task.yield()
    }
    try await Task.sleep(for: .milliseconds(20))
}

private func makeStreams(workers: StubWorkerManager) -> TrUAPIPocketHandler {
    TrUAPIPocketHandler(productId: "game.paseo", workers: workers)
}
