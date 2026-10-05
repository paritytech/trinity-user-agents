import Foundation
import Testing
import ChainRegistry
import SubstrateSdk
import TrUAPIHost
import Products
@testable import polkadot_app

// MARK: - Helpers

private let testProduct = ProductExecutionConfig(productId: "host.product", executionKind: .app)

private func makeHostBridge(
    chainRegistry: ChainRegistryProtocol = MockChainRegistry(),
    confirmationPresenter: MockConfirmationPresenter = MockConfirmationPresenter()
) -> RustHostRuntimeBridge {
    let chainConnections = TrUAPIChainConnectionPool(
        engineResolver: { genesisHash in
            chainRegistry.getChainByGenesis(for: genesisHash.toHex()).flatMap { chain in
                chainRegistry.getConnection(for: chain.chainId)
            }
        },
        logger: Logger.shared
    )
    return RustHostRuntimeBridge(
        chainRegistry: chainRegistry,
        secretStorage: TestHostSecrets(),
        walletId: "test",
        permissionRequester: MockPermissionGuard(),
        osPermissionAsker: MockOSPermissionAsker(),
        chainConnections: chainConnections,
        confirmationPresenter: confirmationPresenter,
        logger: Logger.shared
    )
}

// MARK: - Tests

struct RustHostRuntimeBridgeTests {
    /// Known genesis WITH an app-managed connection → supported (aligned with
    /// chain_connect); the host-level bridge answers this before any product.
    @Test func featureSupportedKnownChainWithConnectionReturnsTrue() async throws {
        let genesis = Data(repeating: 0xAB, count: 32)
        let chainRegistry = MockChainRegistry()
        let remoteChain = ChainMock.makeRemoteChain(name: "TestChain")
        let chainModel = ChainMock.makeChainModel(from: remoteChain, order: 0)
        chainRegistry.chainsByGenesis[genesis.toHex()] = chainModel
        chainRegistry.connectionsByChainId[chainModel.chainId] = MockChainConnection()
        let bridge = makeHostBridge(chainRegistry: chainRegistry)

        let result = try await bridge.featureSupported(request: .chain(genesisHash: genesis))
        #expect(result)
    }

    @Test func featureSupportedUnknownChainReturnsFalse() async throws {
        let bridge = makeHostBridge()

        let result = try await bridge.featureSupported(
            request: .chain(genesisHash: Data(repeating: 0xFF, count: 32))
        )
        #expect(!result)
    }

    /// No product identity at host level: navigation is rejected.
    @Test func navigateToRejects() async {
        let bridge = makeHostBridge()

        await #expect(throws: HostNavigateToError.self) {
            try await bridge.navigateTo(url: "https://example.com")
        }
    }

    /// Core-reviewed actions delegate to the injected confirmation presenter
    /// at host level, just like the per-execution bridge.
    @Test func confirmUserActionDelegatesToPresenter() async throws {
        let presenter = MockConfirmationPresenter()
        presenter.verdictToReturn = true
        let bridge = makeHostBridge(confirmationPresenter: presenter)

        let review = UserConfirmationReview.accountAccess(
            AccountAccessReview(requestingProductId: "a.dot", targetProductId: "b.dot")
        )
        let result = try await bridge.confirmUserAction(review: review)

        #expect(result)
        #expect(presenter.receivedReview == review)
        #expect(presenter.receivedRequesterName == "host")
    }

    @Test func workerPermissionsUseIndependentPromptAndOSGate() async throws {
        let bridge = makeHostBridge()

        await #expect(throws: HostRejection.self) {
            try await bridge.devicePermission(product: testProduct, request: .camera)
        }
        let remote = try await bridge.remotePermission(product: testProduct, request: .webRtc)

        #expect(remote == .allowAlways)
    }

    @Test(arguments: [TrUAPIPermissionDecision.allowOnce, .allowAlways, .deny])
    func confirmPermissionPreservesLifetime(decision: TrUAPIPermissionDecision) async throws {
        let presenter = MockConfirmationPresenter()
        presenter.permissionDecisionToReturn = decision
        let bridge = makeHostBridge(confirmationPresenter: presenter)
        let review = UserConfirmationReview.identityDisclosure(
            IdentityDisclosureReview(productId: "caller.dot")
        )

        let result = try await bridge.confirmPermission(review: review)

        #expect(result == decision)
        #expect(presenter.receivedReview == review)
        #expect(presenter.receivedRequesterName == "host")
    }


}

// MARK: - Runtime config

struct TrUAPIHostRuntimeProviderConfigTests {
    /// Missing chains fail explicitly rather than degrading — the config seam
    /// throws when the registry cannot resolve the required chains.
    @Test func makeRuntimeConfigThrowsWhenChainsMissing() {
        #expect(throws: (any Error).self) {
            _ = try TrUAPIHostRuntimeProvider.makeRuntimeConfig(
                chainRegistry: MockChainRegistry(),
                networkSuffix: "paseo",
                databaseDirectory: NSTemporaryDirectory(),
                platformVersion: "test"
            )
        }
    }
}

private actor TestHostSecrets: HostSecretStorageBackend {
    private var values: [String: Data] = [:]
    func read(key: SecretCoreStorageKey) async throws -> Data? { values[secretCoreStorageKeyIdentifier(key: key)] }
    func write(key: SecretCoreStorageKey, value: Data) async throws { values[secretCoreStorageKeyIdentifier(key: key)] = value }
    func clear(key: SecretCoreStorageKey) async throws { values[secretCoreStorageKeyIdentifier(key: key)] = nil }
}
