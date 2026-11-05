import Foundation
import Testing
import UIKit
import UIKitExt
@testable import Products

@MainActor
struct ProductPermissionRequesterTests {
    @Test
    func aPromptThatCannotBeShownFailsInsteadOfDeciding() async {
        let requester = ProductPermissionRequester(router: StubPermissionRouter(decision: nil))

        await #expect(throws: ProductPermissionPromptError.notPresentable) {
            try await requester.prompt(productId: "game.paseo", permission: .deviceCapability(.notifications))
        }
    }

    @Test
    func aShownPromptAnswersWithTheUsersDecision() async throws {
        let requester = ProductPermissionRequester(router: StubPermissionRouter(decision: .allowOnce))

        let decision = try await requester.prompt(productId: "game.paseo", permission: .balanceAccess)

        #expect(decision == .allowOnce)
    }
}

@MainActor
private final class StubPermissionRouter: ProductPermissionRouting {
    private let decision: PermissionDecision?

    init(decision: PermissionDecision?) {
        self.decision = decision
    }

    func setPresentationView(_: ControllerBackedProtocol) {}
    var isReady: Bool { decision != nil }
    func present(view _: ControllerBackedProtocol) -> Bool { decision != nil }

    func showPrompt(context: ProductPermissionContext) {
        if let decision {
            context.deliver(decision)
        } else {
            context.fail(ProductPermissionPromptError.notPresentable)
        }
    }
}
