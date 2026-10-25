import Foundation
import Products
import Testing

@testable import polkadot_app

@Suite("ProductPermissionRequesterFactory")
@MainActor
struct ProductPermissionRequesterFactoryTests {
    private let notifications = ProductPermission.deviceCapability(.notifications)

    private func makeRequester() -> ProductPermissionRequesting {
        ProductPermissionRequesterFactory.create(router: ProductsRouter(), fundingProvider: NoFundingProvider())
    }

    @Test("a blessed product's device permission is granted without a screen to prompt on")
    func blessedProductNeedsNoPrompt() async throws {
        let decision = try await makeRequester().prompt(productId: "dim2.testnet", permission: notifications)

        #expect(decision == .allowAlways)
    }

    @Test("an unknown product still needs the prompt")
    func unknownProductStillPrompts() async {
        await #expect(throws: ProductPermissionPromptError.notPresentable) {
            try await makeRequester().prompt(productId: "game.testnet", permission: notifications)
        }
    }
}

private struct NoFundingProvider: FundingDomainProviding {
    struct Unavailable: Error {}

    func fundingPage() async throws -> ProductPage { throw Unavailable() }
    func offrampPage() async throws -> ProductPage { throw Unavailable() }
    func fundingLabels() -> Set<String> { [] }
}
