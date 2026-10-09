import Foundation
import Testing
import ChainRegistry
import SubstrateSdk
import TrUAPIHost
@testable import polkadot_app

// MARK: - Helpers

private let testProduct = ProductExecutionConfig(productId: "host.product", executionKind: .app)

private func makeHostDefaults() -> UserDefaults {
    UserDefaults(suiteName: "io.polkadotapp.tests.truapi-host-bridge") ?? .standard
}

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
        coreStorage: TrUAPILocalStorage.createCoreLocalStorage(defaults: makeHostDefaults()),
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

    @Test func permissionsDenyAtHostLevel() async throws {
        let bridge = makeHostBridge()

        let device = try await bridge.devicePermission(product: testProduct, request: .camera)
        let remote = try await bridge.remotePermission(product: testProduct, request: .webRtc)

        #expect(device == .deny)
        #expect(remote == .deny)
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

    /// Core storage is the real host-global backend: writes round-trip.
    @Test func coreStorageRoundTrips() throws {
        let bridge = makeHostBridge()
        let key = Data([0x0A, 0x0B])
        let value = Data([0x10, 0x20, 0x30])

        try bridge.coreStorage.write(key: key, value: value)
        #expect(try bridge.coreStorage.read(key: key) == value)
    }

    /// Product KV has no host-level scope: reads miss and writes are dropped.
    @Test func productStorageIsEmptyAtHostLevel() throws {
        let bridge = makeHostBridge()

        try bridge.storage.write(key: "k", value: Data([0x01]))
        #expect(try bridge.storage.read(key: "k") == nil)
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
                secret: Data([0x01]),
                liteUsername: nil,
                networkSuffix: "paseo",
                databaseDirectory: NSTemporaryDirectory()
            )
        }
    }
}
