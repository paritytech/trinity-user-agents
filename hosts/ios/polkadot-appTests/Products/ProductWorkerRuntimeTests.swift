import Foundation
import Products
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
@testable import polkadot_app

private let gameProductId = try! ProductId("game.paseo")

/// A render that follows a worker is opened again whenever it stops, and the
/// wait before that is time a card spends on a face its product cannot update.
struct ProductWorkerRuntimeTests {
    /// A render that drew was working. A replaced connection fails it on a
    /// healthy worker, so it is opened again after the shortest wait however
    /// often that happened before. A wait that grew with every drop would
    /// freeze the card for half a minute.
    @Test
    func reopensARenderThatDrewAfterTheShortestWait() async throws {
        let workers = StubWorkerRuntime()
        let execution = MockProductExecution()
        execution.renderNodes = [.string(text: "face")]
        execution.keepsRenderStreamOpen = true
        workers.publish([gameProductId: execution])
        let clock = ManualClock()
        let request = ProductRendererRenderRequest(context: .pocketCard(cardId: "loyalty"), payload: Data())
        let faces = workers.renderFollowingWorker(gameProductId, request, logger: MockLogger(), clock: clock) { $0 }
        let drawing = Task { for try await _ in faces {} }
        defer { drawing.cancel() }

        for drop in 0 ..< 3 {
            try await withTimeout(.hangGuard) { await execution.awaitRenderRequest() }
            execution.renders[drop].observer.onError(reason: "the product connection closed")
            try await withTimeout(.hangGuard) { await clock.resumeSleep(for: .seconds(1)) }
        }
    }
}
