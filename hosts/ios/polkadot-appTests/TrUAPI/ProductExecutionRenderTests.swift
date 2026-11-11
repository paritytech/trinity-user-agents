import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// One rule decides how a product is asked to draw. A card and a chat cell
/// both open their renders through it: the execution is asked to say when its
/// product is attached, and the product is then asked once. Whatever that one
/// render answers is the product's own answer.
struct ProductExecutionRenderTests {
    /// A worker connects and only then answers renders. Asking before the
    /// execution reports it attached is refused, so the wait comes first.
    @Test
    func waitsForTheProductBeforeTheOneRender() async throws {
        let execution = MockProductExecution()
        execution.refusesRendersUntilWaited = true

        _ = try await execution.renderWhenConnected(request, until: .now + .seconds(30))

        #expect(execution.renderRequests.count == 1)
    }

    /// Anything the product answers is raised as is: asking again cannot
    /// change it, and waiting one out leaves the card blank for as long as the
    /// host is willing to wait.
    @Test
    func raisesTheAnswerTheProductGave() async {
        let execution = MockProductExecution()
        execution.renderErrors = Array(repeating: ProductRuntimeError.Denied, count: 10)

        await #expect(throws: ProductRuntimeError.Denied) {
            _ = try await execution.renderWhenConnected(request, until: .now + .seconds(30))
        }

        #expect(execution.renderRequests.count == 1)
    }

    /// Once the wait has returned, a product that is still not attached is
    /// not asked on a loop: the one refusal is the answer.
    @Test
    func givesUpOnAProductThatNeverAttaches() async {
        let execution = MockProductExecution()
        execution.renderErrors = Array(repeating: ProductRuntimeError.NotConnected, count: 100)

        await #expect(throws: ProductRuntimeError.NotConnected) {
            _ = try await execution.renderWhenConnected(request, until: .now)
        }

        #expect(execution.renderRequests.count == 1)
    }

    /// The core bounds the wait because the host cannot cancel it, so the
    /// deadline is handed over as the time left: none once it has passed.
    @Test
    func asksTheExecutionToWaitOnlyUntilTheDeadline() async throws {
        let execution = MockProductExecution()

        _ = try await execution.renderWhenConnected(request, until: .now)

        #expect(execution.waits == [0])
    }

    @Test
    func asksTheExecutionForTheTimeLeftUntilTheDeadline() async throws {
        let execution = MockProductExecution()

        _ = try await execution.renderWhenConnected(request, until: .now + .seconds(30))

        let wait = try #require(execution.waits.first)
        #expect((1 ... 30_000).contains(wait))
    }
}

private let request = ProductRendererRenderRequest(
    context: .pocketCard(cardId: "loyalty"),
    payload: Data()
)
