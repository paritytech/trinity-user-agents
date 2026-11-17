import Foundation
import Testing
import ChainRegistry
import Keystore_iOS
import SubstrateSdk
import TrUAPIHost
@testable import polkadot_app

// MARK: - Helpers

private let testProduct = ProductExecutionConfig(productId: try! ProductId("host.product.dot"), executionKind: .app)

private func makeHostDefaults() -> UserDefaults {
    UserDefaults(suiteName: "io.polkadotapp.tests.truapi-host-bridge") ?? .standard
}

private func makeHostBridge(
    chainRegistry: ChainRegistryProtocol = MockChainRegistry(),
    confirmationPresenter: MockConfirmationPresenter = MockConfirmationPresenter()
) -> RustHostRuntimeBridge {
    let chainConnections = TrUAPIV2SpecConnectionPool(
        chainRegistry: chainRegistry,
        applicationHandler: MockApplicationHandler(),
        dateProvider: MockDateProvider(),
        logger: Logger.shared
    )
    return RustHostRuntimeBridge(
        chainRegistry: chainRegistry,
        coreStorage: TrUAPILocalStorage.createCoreLocalStorage(defaults: makeHostDefaults()),
        chainConnections: chainConnections,
        confirmationPresenter: confirmationPresenter,
        backedUpInstallation: TrUAPIBackedUpInstallation(),
        encryptedStorage: TrUAPIEncryptedStorage(keychain: InMemoryKeychain()),
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
            AccountAccessReview(requestingProductId: try! ProductId("a.dot"), targetProductId: try! ProductId("b.dot"))
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
            IdentityDisclosureReview(productId: try! ProductId("caller.dot"))
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

private func makeDatabaseDirectory() throws -> String {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory.path
}

struct TrUAPIHostRuntimeProviderConfigTests {
    /// The core verifies the bundled descriptor under the build's stack link
    /// before it starts, so a config the core refuses leaves the app with no
    /// runtime and no product can open.
    @Test func runtimeStartsFromTheAssembledConfig() throws {
        let config = try TrUAPIHostRuntimeProvider.makeRuntimeConfig(
            secret: Data(repeating: 7, count: 32),
            liteUsername: nil,
            networkSuffix: "testnet",
            databaseDirectory: makeDatabaseDirectory()
        )

        _ = try TrUAPIHostRuntime(bridge: makeHostBridge(), runtimeConfig: config)
    }

    /// Tests run in one build only, so the stacks the other builds run are
    /// started here as well: a copy refreshed without its link, or a
    /// reserved-values flag that refuses its stack, fails before it ships.
    @Test(arguments: [TrUAPIBundledStack.previewnet, TrUAPIBundledStack.paseoNextV2, TrUAPIBundledStack.polkadot])
    func everyBundledStackStartsTheCore(stack: TrUAPIBundledStack.Definition) throws {
        let config = try HostRuntimeConfig(
            hostName: "Polkadot App",
            stack: stack.load(),
            databaseDirectory: makeDatabaseDirectory()
        )

        _ = try TrUAPIHostRuntime(bridge: makeHostBridge(), runtimeConfig: config)
    }
}
