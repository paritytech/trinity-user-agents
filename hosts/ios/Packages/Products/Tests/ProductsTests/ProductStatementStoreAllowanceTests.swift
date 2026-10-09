import Foundation
import Individuality
import KeyDerivation
import SubstrateSdk
import Testing
import UIKitExt
@testable import Products

@MainActor
struct ProductStatementStoreAllowanceTests {
    @Test
    func selectedProductAccountIsFundedWithoutExportingItsKey() async throws {
        let entropy = AllocationTestEntropy()
        let prompt = AllocationTestPrompt()
        let ledger = AllocationTestLedger()
        let manager = ProductsAccountManager(
            entropyManager: entropy,
            allowanceSupport: AllowanceSupport(
                allowancePromptRouter: prompt,
                sssManager: ledger,
                bulletInManager: ledger,
                smartContractManager: ledger
            )
        )
        // Rust AllocatableResource::ProductStatementStoreAllowance(Index(7)).
        let request = try AllocatableResource.fromScaleEncoded(Data([4, 0, 7, 0, 0, 0]))
        let expectedAccount = try ProductAccountHolder(entropyManager: entropy).deriveAccount(
            ProductAccountId(productId: "chat.dot", derivationIndex: .index(7))
        )

        let outcomes = try await manager.requestResourceAllocation(
            for: "chat.dot", resources: [request], policy: .increase
        )
        #expect(await ledger.accounts == [expectedAccount])
        let outcome = try #require(outcomes.first)
        // SSO Allocated(ProductStatementStoreAllowance), with no secret payload.
        #expect(try outcome.scaleEncoded() == Data([0, 4]))
        #expect(try JSONSerialization.jsonObject(with: JSONEncoder().encode(outcome)) as? [String: String]
            == ["kind": "Allocated"])

        prompt.decision = .rejected
        let denied = try await manager.requestResourceAllocation(
            for: "chat.dot", resources: [request], policy: .increase
        )
        #expect(denied == [.rejected])
        #expect(await ledger.accounts == [expectedAccount])
    }
}

private struct AllocationTestEntropy: RootEntropyManaging {
    func fetchRootEntropy() throws -> Data { Data(repeating: 0xAB, count: 16) }
    func createRootEntropy(_: Data) throws {}
    func hasRootEntropy() throws -> Bool { true }
}

private actor AllocationTestLedger: AllowanceManaging {
    private(set) var accounts: [AccountId] = []

    func allocate(
        accountId: AccountId,
        policy _: OnExistingAllowancePolicy,
        priority _: AllowanceRecord.Priority
    ) async throws {
        accounts.append(accountId)
    }
}

@MainActor
private final class AllocationTestPrompt: AllowancePromptRouting {
    var decision: AllowancePromptDecision = .approved
    var isReady: Bool { true }

    func setPresentationView(_: ControllerBackedProtocol) {}
    func present(view _: ControllerBackedProtocol) -> Bool { false }
    func showAllowancePrompt(context: AllowancePromptContext) {
        context.deliver(decision)
    }
}
