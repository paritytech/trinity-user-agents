import Foundation
import Testing
import UIKit
import UIKitExt
import Products

@testable import polkadot_app

@MainActor
struct PromptPresentationScopeTests {
    private final class StubView: UIViewController, ControllerBackedProtocol {}

    private final class PresentingController: UIViewController {
        private(set) var dismissCount = 0

        override func dismiss(animated _: Bool, completion: (() -> Void)? = nil) {
            dismissCount += 1
            completion?()
        }
    }

    private final class PresentedController: UIViewController {
        weak var presenter: UIViewController?
        var above: UIViewController?

        override var presentingViewController: UIViewController? { presenter }
        override var presentedViewController: UIViewController? { above }
    }

    @Test("Withdrawing dismisses a prompt still on screen")
    func withdrawDismissesPresentedPrompt() {
        let presenter = PresentingController()
        let prompt = PresentedController()
        prompt.presenter = presenter
        let scope = PromptPresentationScope()

        #expect(scope.admit(prompt))
        scope.withdraw()

        #expect(presenter.dismissCount == 1)
    }

    @Test("Withdrawing leaves a prompt up while another is presented above it")
    func withdrawSparesPromptsAbove() {
        let presenter = PresentingController()
        let prompt = PresentedController()
        prompt.presenter = presenter
        prompt.above = UIViewController()
        let scope = PromptPresentationScope()

        #expect(scope.admit(prompt))
        scope.withdraw()

        #expect(presenter.dismissCount == 0)
    }

    @Test("A withdrawn scope refuses later prompts")
    func withdrawnScopeRefusesPrompts() {
        let scope = PromptPresentationScope()

        scope.withdraw()

        #expect(!scope.admit(UIViewController()))
    }

    @Test("ProductsRouter does not present under a withdrawn scope")
    func routerRefusesPresentingInWithdrawnScope() async {
        let anchor = StubView()
        let router = ProductsRouter()
        router.setPresentationView(anchor)
        let scope = PromptPresentationScope()
        scope.withdraw()

        let presented = await scope.bind {
            router.present(view: StubView())
        }

        #expect(!presented)
    }

    @Test("Cancelling the task a scope runs in withdraws it")
    func cancellingRunWithdrawsScope() async {
        let scope = PromptPresentationScope()
        let task = Task {
            await scope.run {
                try? await Task.sleep(for: .seconds(60))
            }
        }

        task.cancel()
        await task.value
        for _ in 0 ..< 100 where !scope.isWithdrawn {
            await Task.yield()
        }

        #expect(scope.isWithdrawn)
    }

    @Test("A dismissed allowance prompt resolves as rejected", .timeLimit(.minutes(1)))
    func dismissedAllowancePromptRejects() async {
        let decision: AllowancePromptDecision = await withCheckedContinuation { continuation in
            var context: AllowancePromptContext? = AllowancePromptContext(productId: "test.product", resources: [])
            context?.setContinuation(continuation)
            context = nil
        }

        #expect(decision == .rejected)
    }

    @Test("A dismissed permission prompt resolves as denied", .timeLimit(.minutes(1)))
    func dismissedPermissionPromptDenies() async {
        let decision: PermissionDecision = await withCheckedContinuation { continuation in
            var context: ProductPermissionContext? = ProductPermissionContext(
                productId: "test.product",
                permissions: []
            )
            context?.setContinuation(continuation)
            context = nil
        }

        #expect(decision == .deny)
    }

    @Test("A dismissed create-proof prompt resolves as rejected", .timeLimit(.minutes(1)))
    func dismissedCreateProofPromptRejects() async {
        let request = CreateProofConfirmationRequest(
            callingProductId: nil,
            onBehalfOfProductId: "test.product",
            suffix: Data(),
            message: Data()
        )
        let decision: CreateProofDecision = await withCheckedContinuation { continuation in
            var context: CreateProofConfirmationContext? = CreateProofConfirmationContext(request: request)
            context?.setContinuation(continuation)
            context = nil
        }

        #expect(decision == .rejected)
    }
}
