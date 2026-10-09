import Foundation
import Testing
import UIKitExt
import Products
import Individuality

@testable import polkadot_app

@MainActor
struct ProductsAccountManagerAllocationTests {
    /// Approves every allowance prompt, running `onPrompt` first.
    private final class ApprovingAllowanceRouter: AllowancePromptRouting {
        var onPrompt: () -> Void = {}

        func setPresentationView(_: ControllerBackedProtocol) {}

        var isReady: Bool { true }

        func present(view _: ControllerBackedProtocol) -> Bool { true }

        func showAllowancePrompt(context: AllowancePromptContext) {
            onPrompt()
            context.deliver(.approved)
        }
    }

    @Test("A request cancelled at its prompt starts no allocation step")
    func cancelledRequestAllocatesNothing() async {
        let router = ApprovingAllowanceRouter()
        let statementStoreManager = MockAllowanceManager()
        let manager = ProductsAccountManager(
            entropyManager: SsoTestData.entropyManager,
            allowanceSupport: AllowanceSupport(
                allowancePromptRouter: router,
                sssManager: statementStoreManager,
                bulletInManager: MockAllowanceManager(),
                smartContractManager: MockAllowanceManager()
            )
        )

        let task = Task {
            try await manager.requestResourceAllocation(
                for: "test.product",
                resources: [.statementStoreAllowance],
                policy: .ignore
            )
        }
        router.onPrompt = { task.cancel() }

        let result = await task.result

        #expect(throws: CancellationError.self) { try result.get() }
        #expect(statementStoreManager.allocateCallCount == 0)
    }
}
