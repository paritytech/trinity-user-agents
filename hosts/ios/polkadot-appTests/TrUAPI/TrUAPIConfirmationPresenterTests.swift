import Foundation
import Testing
import UIKit
import UIKitExt
import Products
import TrUAPIHost

@testable import polkadot_app

@MainActor
struct TrUAPIConfirmationPresenterTests {
    /// Anchor that records the prompt scope bound when a prompt is presented,
    /// without presenting anything.
    private final class ScopeCapturingAnchor: UIViewController, ControllerBackedProtocol {
        private(set) var presentedScope: PromptPresentationScope?
        var onPresent: () -> Void = {}

        override func present(_: UIViewController, animated _: Bool, completion _: (() -> Void)? = nil) {
            presentedScope = PromptPresentationScope.current
            onPresent()
        }
    }

    @Test("Cancelling a confirmation withdraws the prompt it presented", .timeLimit(.minutes(1)))
    func cancellationClosesPresentedPrompt() async throws {
        let anchor = ScopeCapturingAnchor()
        let routers = ProductRoutersFacade.sso()
        routers.setPresentationView(anchor)
        let presenter = TrUAPIConfirmationPresenter(routerFacade: routers, logger: MockLogger())
        let (presented, presentedContinuation) = AsyncStream<Void>.makeStream()
        anchor.onPresent = { presentedContinuation.yield() }

        let task = Task {
            await presenter.confirm(
                review: .productSubtree(ProductSubtreeReview(productId: "test.product")),
                from: "test.product"
            )
        }
        var presentations = presented.makeAsyncIterator()
        await presentations.next()
        let scope = try #require(anchor.presentedScope)
        #expect(!scope.isWithdrawn)

        task.cancel()
        let verdict = await task.value

        #expect(!verdict)
        #expect(scope.isWithdrawn)
    }
}
