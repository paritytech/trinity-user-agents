import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// One rule decides what is worth asking a product again for. A card and a chat
/// cell both open their renders through it, so a rule that waited out an answer
/// the product meant would hold either of them blank.
///
/// Driven by the answers the execution gives rather than by elapsed time: what
/// matters is which answers are asked again, not how long a host is willing to
/// wait.
struct ProductExecutionRenderTests {
    /// A worker connects and only then subscribes, so its first renders are
    /// refused while it is still coming up. Those are the ones worth waiting
    /// out.
    @Test
    func asksAgainWhileTheWorkerIsStillComingUp() async throws {
        let execution = MockProductExecution()
        execution.renderErrors = [ProductRuntimeError.NotConnected, ProductRuntimeError.NotConnected]

        _ = try await execution.renderWhenConnected(request, until: .now + .seconds(30), retryEvery: .zero)

        #expect(execution.renderRequests.count == 3)
    }

    /// Anything else is the product's own answer, and asking again cannot
    /// change it. Waiting one out leaves the card blank for as long as the host
    /// is willing to wait.
    @Test
    func raisesAnAnswerTheProductMeant() async {
        let execution = MockProductExecution()
        execution.renderErrors = Array(repeating: ProductRuntimeError.Denied, count: 10)

        await #expect(throws: ProductRuntimeError.Denied) {
            _ = try await execution.renderWhenConnected(request, until: .now + .seconds(30), retryEvery: .zero)
        }

        #expect(execution.renderRequests.count == 1)
    }

    /// A worker that never subscribes must not be asked forever: the deadline
    /// is what ends it, and the refusal it ends on is the product's.
    @Test
    func givesUpOnAWorkerThatNeverSubscribes() async {
        let execution = MockProductExecution()
        execution.renderErrors = Array(repeating: ProductRuntimeError.NotConnected, count: 100)

        await #expect(throws: ProductRuntimeError.NotConnected) {
            _ = try await execution.renderWhenConnected(request, until: .now, retryEvery: .zero)
        }

        #expect(execution.renderRequests.count == 1)
    }
}

private let request = ProductRendererRenderRequest(
    context: .pocketCard(cardId: "loyalty"),
    payload: Data()
)
