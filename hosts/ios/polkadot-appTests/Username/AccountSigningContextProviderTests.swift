import Foundation
import Testing
import Individuality
@testable import polkadot_app

struct AccountSigningContextProviderTests {
    @Test func contextsAreNilWhenRuntimesPublishNone() async throws {
        let chainRegistry = MockChainRegistry()
        chainRegistry.runtimeProviders = [
            "people": Self.runtimeProvider(),
            "asset-hub": Self.runtimeProvider()
        ]
        let provider = AccountSigningContextProvider(
            peopleChainId: "people",
            assetHubChainId: "asset-hub",
            chainRegistry: chainRegistry
        )

        let contexts = try await provider.fetchContexts()

        #expect(contexts.peopleLite == nil)
        #expect(contexts.dotnsGateway == nil)
    }
}

private extension AccountSigningContextProviderTests {
    static func runtimeProvider() -> MockRuntimeProvider {
        let runtimeProvider = MockRuntimeProvider()
        runtimeProvider.coderFactory = MockCoderFactory()

        return runtimeProvider
    }
}
