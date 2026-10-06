import Foundation
import Testing
import ChainRegistry
import Products
import SubstrateSdk
import Keystore_iOS
import TrUAPIHost
import DesignSystem
import UIKit

// Scoped import: the TrUAPIHost module also declares a *type* named TrUAPIHost,
// so `TrUAPIHost.ProductAccountId` cannot disambiguate from Products'.
import struct TrUAPIHost.ProductAccountId
import UIKitExt
@testable import polkadot_app

// MARK: - Helpers

private let testProduct = ProductExecutionConfig(productId: "test.product", executionKind: .app)

private func makeRegistryPool(chainRegistry: ChainRegistryProtocol) -> TrUAPIChainConnectionPool {
    TrUAPIChainConnectionPool(
        engineResolver: { genesisHash in
            chainRegistry.getChainByGenesis(for: genesisHash.toHex()).flatMap { chain in
                chainRegistry.getConnection(for: chain.chainId)
            }
        },
        logger: Logger.shared
    )
}

private func makeTestDefaults() -> UserDefaults {
    UserDefaults(suiteName: "io.polkadotapp.tests.truapi-bridge") ?? .standard
}

// MARK: - Stubs

private struct StubHostProvider: ProductHostProviding {
    func host(rawString _: String) -> ProductHost? {
        nil
    }

    func host(url _: URL) -> ProductHost? {
        nil
    }

    func host(navigationDestination _: String) -> ProductHost? {
        nil
    }

    func page(url _: URL) -> ProductPage? {
        nil
    }

    func page(navigationDestination _: String) -> ProductPage? {
        nil
    }

    func host(label _: String) -> ProductHost? {
        nil
    }

    func resolveHost(label _: String) async throws -> ProductHost? {
        nil
    }

    func resolveHost(rawString _: String) async throws -> ProductHost? {
        nil
    }

    func resolvePage(destination _: String) async throws -> ProductPage? {
        nil
    }
}

@MainActor
private final class MockThemeManager: ThemeManagerProtocol {
    private(set) var theme: Theme
    private(set) var mode: ThemeMode
    private var observers: [UUID: AsyncStream<Theme>.Continuation] = [:]
    private var terminationWaiter: CheckedContinuation<Void, Never>?

    init(selection: ThemeSelection = .berlinNight) {
        mode = .app(selection)
        theme = ThemesRegistry.makeTheme(selection)
    }

    func observeTheme() -> AsyncStream<Theme> {
        AsyncStream { continuation in
            let id = UUID()
            observers[id] = continuation
            continuation.yield(theme)
            continuation.onTermination = { [weak self] _ in
                Task { @MainActor in
                    guard let self else { return }
                    observers[id] = nil
                    if observers.isEmpty {
                        terminationWaiter?.resume()
                        terminationWaiter = nil
                    }
                }
            }
        }
    }

    func select(_ mode: ThemeMode) {
        guard mode != self.mode else { return }
        self.mode = mode
        switch mode {
        case let .app(selection):
            theme = ThemesRegistry.makeTheme(selection)
        }
        observers.values.forEach { $0.yield(theme) }
    }

    func setup(scene _: UIWindowScene) {}

    func waitForObservationToEnd() async {
        guard !observers.isEmpty else { return }
        await withCheckedContinuation { terminationWaiter = $0 }
    }
}

// MARK: - Bridge factory

@MainActor
private func makeBridge(
    productId: String = "test.product",
    permissionGuard: MockPermissionGuard = MockPermissionGuard(),
    osPermissionAsker: MockOSPermissionAsker = MockOSPermissionAsker(),
    notificationScheduler: MockNotificationScheduler = MockNotificationScheduler(),
    chainRegistry: MockChainRegistry = MockChainRegistry(),
    confirmationPresenter: MockConfirmationPresenter = MockConfirmationPresenter(),
    preimageCache: TrUAPIPreimageCache = TrUAPIPreimageCache { _ in nil },
    productStorageFails: Bool = false,
    hostProvider: ProductHostProviding = StubHostProvider(),
    themeManager: ThemeManagerProtocol? = nil,
    chainConnections: TrUAPIChainConnecting? = nil
) -> RustProductExecutionBridge {
    let router = MockNavigationRouter()
    let pool = makeRegistryPool(chainRegistry: chainRegistry)
    let productStorage: TrUAPILocalStoring = productStorageFails
        ? FailingProductStorage()
        : TrUAPILocalStorage.createProductLocalStorage(
            productId: productId,
            defaults: makeTestDefaults()
        )
    return RustProductExecutionBridge(dependencies: .init(
        productId: productId,
        permissionGuard: permissionGuard,
        osPermissionAsker: osPermissionAsker,
        notificationScheduler: notificationScheduler,
        navigationRouter: router,
        chainRegistry: chainRegistry,
        chainConnections: chainConnections ?? pool,
        productStorage: productStorage,
        coreStorage: TrUAPILocalStorage.createCoreLocalStorage(defaults: makeTestDefaults()),
        confirmationPresenter: confirmationPresenter,
        chatFiles: UnavailableNativeChatFiles(),
        preimageCache: preimageCache,
        hostProvider: hostProvider,
        themeManager: themeManager ?? MockThemeManager(),
        logger: Logger.shared
    ))
}

// MARK: - Tests

/// MainActor suite: MockNavigationRouter is @MainActor, and Swift Testing
/// does not run tests on the main thread by default.
@MainActor
struct RustRuntimeBridgeTests {
    // MARK: devicePermission

    /// `devicePermission(.camera)` routes to
    /// `permissionGuard.requestDevicePermissionDecision(productId:capability:.camera)`
    /// and returns its verdict (async callback — awaited directly).
    @Test func devicePermissionRoutesToGuard() async throws {
        let guard_ = MockPermissionGuard()
        guard_.verdictToReturn = true
        let bridge = makeBridge(productId: "cam.product", permissionGuard: guard_)

        let result = try await bridge.devicePermission(product: testProduct, request: .camera)

        #expect(result == .allowAlways)
        #expect(guard_.requestedProductId == "cam.product")
        #expect(guard_.requestedPermission == .deviceCapability(.camera))
    }

    @Test func devicePermissionDenied() async throws {
        let guard_ = MockPermissionGuard()
        guard_.verdictToReturn = false
        let bridge = makeBridge(permissionGuard: guard_)

        let result = try await bridge.devicePermission(product: testProduct, request: .notifications)

        #expect(result == .deny)
        #expect(guard_.requestedPermission == .deviceCapability(.notifications))
    }

    @Test func devicePermissionStatusReadsOSWithoutPrompting() async throws {
        for request in [HostDevicePermissionRequest.camera, .microphone, .notifications] {
            for status in [OSPermissionStatus.allowed, .denied, .notDetermined] {
                let osAsker = MockOSPermissionAsker()
                osAsker.statusToReturn = status
                let guard_ = MockPermissionGuard()
                let bridge = makeBridge(permissionGuard: guard_, osPermissionAsker: osAsker)
                let expected: DevicePermissionStatus = switch status {
                case .allowed: .granted
                case .denied: .denied
                case .notDetermined: .notDetermined
                }

                #expect(try await bridge.devicePermissionStatus(request: request) == expected)
                #expect(osAsker.checkedCapabilities == [request.deviceCapabilityType])
                #expect(osAsker.requestedCapabilities.isEmpty)
                #expect(guard_.requestedPermission == nil)
            }
        }
    }

    @Test(arguments: [
        (HostDevicePermissionRequest.openUrl, DevicePermissionStatus.notApplicable),
        (.bluetooth, .notApplicable),
        (.nfc, .notApplicable),
        (.location, .notDetermined),
        (.clipboard, .notApplicable),
        (.biometrics, .notApplicable),
    ])
    func devicePermissionStatusWithoutOSQuery(
        request: HostDevicePermissionRequest,
        expected: DevicePermissionStatus
    ) async throws {
        let osAsker = MockOSPermissionAsker()
        let bridge = makeBridge(osPermissionAsker: osAsker)

        #expect(try await bridge.devicePermissionStatus(request: request) == expected)
        #expect(osAsker.checkedCapabilities.isEmpty)
        #expect(osAsker.requestedCapabilities.isEmpty)
    }

    // MARK: remotePermission

    /// `remotePermission`: Remote{domains:["a.io"]} maps to
    /// `ProductPermission.networkAccess(domain: "a.io")` batched request.
    @Test(arguments: [Products.PermissionDecision.allowOnce, .allowAlways, .deny])
    func remotePermissionDomains(decision: Products.PermissionDecision) async throws {
        let guard_ = MockPermissionGuard()
        guard_.decisionToReturn = decision
        let bridge = makeBridge(permissionGuard: guard_)

        let result = try await bridge.remotePermission(product: testProduct, request: .remote(domains: ["a.io"]))

        let expected: TrUAPIPermissionDecision = switch decision {
        case .allowOnce: .allowOnce
        case .allowAlways: .allowAlways
        case .deny: .deny
        }
        #expect(result == expected)
        #expect(guard_.requestedBatchedPermissions == [.networkAccess(domain: "a.io")])
    }

    @Test func remotePermissionWebRTC() async throws {
        let guard_ = MockPermissionGuard()
        let bridge = makeBridge(permissionGuard: guard_)

        let result = try await bridge.remotePermission(product: testProduct, request: .webRtc)

        #expect(result == .allowAlways)
        #expect(guard_.requestedBatchedPermissions == [.webRtcAccess])
    }

    /// `remotePermission`: JamPeers asks for access keyed by its genesis as
    /// lowercase `0x` hex, so each network is granted separately.
    @Test func remotePermissionJamPeers() async throws {
        let guard_ = MockPermissionGuard()
        let bridge = makeBridge(permissionGuard: guard_)
        let genesis = Data([0x10, 0xC1, 0x23, 0xF0] + [UInt8](repeating: 0xAB, count: 28))

        let result = try await bridge.remotePermission(
            product: testProduct,
            request: .jamPeers(genesis: genesis)
        )

        #expect(result == .allowAlways)
        #expect(guard_.requestedBatchedPermissions == [
            .jamPeersAccess(genesis: "0x10c123f0" + String(repeating: "ab", count: 28))
        ])
    }

    // MARK: pushNotification

    /// `pushNotification` maps text/deeplink/scheduledAt onto the scheduler
    /// request and returns its UInt32.
    @Test func pushNotificationSchedules() async throws {
        let scheduler = MockNotificationScheduler()
        scheduler.notificationIdToReturn = 99
        let bridge = makeBridge(productId: "push.product", notificationScheduler: scheduler)

        let notificationId = try await bridge.pushNotification(
            request: HostPushNotificationRequest(text: "hi", deeplink: "d", scheduledAt: nil)
        )

        #expect(notificationId == 99)
        #expect(scheduler.scheduledProductId == "push.product")
        #expect(scheduler.scheduledRequest?.text == "hi")
        #expect(scheduler.scheduledRequest?.deeplink == "d")
        #expect(scheduler.scheduledRequest?.scheduledAtMs == nil)
    }

    /// cancelNotification is fire-and-forget (invoked inline on the
    /// dispatcher thread); await the scheduler callback before asserting.
    @Test func cancelNotificationDelegates() async throws {
        let scheduler = MockNotificationScheduler()
        let bridge = makeBridge(notificationScheduler: scheduler)

        await withCheckedContinuation { continuation in
            scheduler.onCancel = { _ in continuation.resume() }
            do {
                try bridge.cancelNotification(id: 77)
            } catch {
                continuation.resume()
                Issue.record("cancelNotification threw: \(error)")
            }
        }

        #expect(scheduler.cancelledNotificationId == 77)
    }

    // MARK: featureSupported

    /// `featureSupported`: known genesis WITH an app-managed connection →
    /// true (aligned with chain_connect); known without connection → false;
    /// unknown → false. Dispatcher-thread path: returns promptly.
    @Test func featureSupportedKnownChainReturnsTrue() async throws {
        let genesis = Data(repeating: 0xAB, count: 32)
        let chainRegistry = MockChainRegistry()
        let remoteChain = ChainMock.makeRemoteChain(name: "TestChain")
        let chainModel = ChainMock.makeChainModel(from: remoteChain, order: 0)
        chainRegistry.chainsByGenesis[genesis.toHex()] = chainModel
        chainRegistry.connectionsByChainId[chainModel.chainId] = MockChainConnection()
        let bridge = makeBridge(chainRegistry: chainRegistry)

        let result = try await bridge.featureSupported(request: .chain(genesisHash: genesis))
        #expect(result)
    }

    @Test func featureSupportedKnownChainWithoutConnectionReturnsFalse() async throws {
        let genesis = Data(repeating: 0xAB, count: 32)
        let chainRegistry = MockChainRegistry()
        let remoteChain = ChainMock.makeRemoteChain(name: "TestChain")
        chainRegistry.chainsByGenesis[genesis.toHex()] = ChainMock.makeChainModel(from: remoteChain, order: 0)
        let bridge = makeBridge(chainRegistry: chainRegistry)

        let result = try await bridge.featureSupported(request: .chain(genesisHash: genesis))
        #expect(!result)
    }

    @Test func featureSupportedUnknownChainReturnsFalse() async throws {
        let bridge = makeBridge(chainRegistry: MockChainRegistry())

        let result = try await bridge.featureSupported(
            request: .chain(genesisHash: Data(repeating: 0xFF, count: 32))
        )
        #expect(!result)
    }

    // MARK: chainConnect

    /// `chainConnect` returns nil for unknown genesis (pool has no matching chain).
    @Test func chainConnectUnknownGenesisReturnsNil() throws {
        let bridge = makeBridge(chainRegistry: MockChainRegistry())

        let connectionId = try bridge.chainConnect(genesisHash: Data(repeating: 0, count: 32))
        #expect(connectionId == nil)
    }

    /// `chainSend` throws for an unknown connection ID. The bridge surfaces the
    /// raw error; the TrUAPIHost `HostCallbackAdapter` maps it to an FFI
    /// `HostRejection` before it reaches the rust core.
    @Test func chainSendUnknownConnectionThrows() {
        let bridge = makeBridge()

        #expect(throws: TrUAPIChainConnectionError.self) {
            try bridge.chainSend(connectionId: 99, request: "{}")
        }
    }

    /// `chainClose` is a no-op for an unknown connection ID (no crash).
    @Test func chainCloseUnknownConnectionNoOp() throws {
        let bridge = makeBridge()
        try bridge.chainClose(connectionId: 99)
    }

    // MARK: confirmUserAction

    /// `confirmUserAction` maps the typed review onto the presentation action
    /// and returns the presenter verdict.
    @Test func confirmUserActionReturnsTrueWhenConfirmed() async throws {
        let presenter = MockConfirmationPresenter()
        presenter.verdictToReturn = true
        let bridge = makeBridge(productId: "confirm.product", confirmationPresenter: presenter)

        let review = UserConfirmationReview.signRaw(
            .legacyAccount(
                request: HostSignRawWithLegacyAccountRequest(signer: "5Ffff", payload: .payload(payload: "hello")),
                watermarked: true
            )
        )
        let result = try await bridge.confirmUserAction(review: review)

        #expect(result)
        #expect(presenter.receivedReview == review)
        #expect(presenter.receivedRequesterName == "confirm.product")
    }

    @Test func confirmUserActionReturnsFalseWhenDenied() async throws {
        let presenter = MockConfirmationPresenter()
        presenter.verdictToReturn = false
        let bridge = makeBridge(confirmationPresenter: presenter)

        let review = UserConfirmationReview.accountAccess(
            AccountAccessReview(requestingProductId: "a.dot", targetProductId: "b.dot")
        )
        let result = try await bridge.confirmUserAction(review: review)

        #expect(!result)
        #expect(presenter.receivedReview == review)
    }

    @Test func confirmUserActionMapsSignVrfReview() async throws {
        let presenter = MockConfirmationPresenter()
        presenter.verdictToReturn = true
        let bridge = makeBridge(confirmationPresenter: presenter)

        let review = UserConfirmationReview.signVrf(
            SignVrfReview(
                callingProductId: "vrf.dot",
                request: HostAccountSignVrfRequest(
                    account: ProductAccountId(
                        dotNsIdentifier: "vrf.dot",
                        derivationIndex: .index(0)
                    ),
                    transcriptLabel: Data("label".utf8),
                    items: [VrfTranscriptItem(label: Data("item".utf8), value: Data([1, 2, 3]))]
                )
            )
        )
        let result = try await bridge.confirmUserAction(review: review)

        #expect(result)
        #expect(presenter.receivedReview == review)
    }

    @Test(arguments: [TrUAPIPermissionDecision.allowOnce, .allowAlways, .deny])
    func confirmPermissionPreservesLifetime(decision: TrUAPIPermissionDecision) async throws {
        let presenter = MockConfirmationPresenter()
        presenter.permissionDecisionToReturn = decision
        let bridge = makeBridge(productId: "caller.dot", confirmationPresenter: presenter)
        let review = UserConfirmationReview.accountAccess(
            AccountAccessReview(requestingProductId: "caller.dot", targetProductId: "target.dot")
        )

        let result = try await bridge.confirmPermission(review: review)

        #expect(result == decision)
        #expect(presenter.receivedReview == review)
        #expect(presenter.receivedRequesterName == "caller.dot")
    }

    // MARK: lookupPreimage

    @Test func lookupPreimageAwaitsFetchOnColdMiss() async throws {
        let stubValue = Data([0xBE, 0xEF])
        let cache = TrUAPIPreimageCache { _ in stubValue }
        let bridge = makeBridge(preimageCache: cache)

        let value = try await bridge.lookupPreimage(key: Data([0x01, 0x02]))

        #expect(value == stubValue)
    }

    // MARK: TrUAPIPreimageCache

    /// When the fetcher returns nil, lookup returns nil, nothing is cached,
    /// and a later lookup retries the fetch.
    @Test func preimageCacheNilFetchReturnsNilAndRetries() async {
        let counter = CallCounter()
        let cache = TrUAPIPreimageCache { _ in
            counter.increment()
            return nil
        }

        let first = await cache.lookup(key: Data([0xAA]))
        let second = await cache.lookup(key: Data([0xAA]))

        #expect(first == nil)
        #expect(second == nil)
        #expect(counter.count == 2)
    }

    /// Concurrent misses for one key coalesce into a single fetch; a later
    /// lookup hits the cache without fetching again. The fetch is gated on an
    /// explicit signal (no wall-clock sleeps): it stays in flight until the
    /// second lookup had a chance to join the coalesced run.
    @Test func preimageCacheCoalescesConcurrentMisses() async {
        let counter = CallCounter()
        let (fetchEntered, enteredContinuation) = AsyncStream<Void>.makeStream()
        let (gate, gateContinuation) = AsyncStream<Void>.makeStream()

        let cache = TrUAPIPreimageCache { _ in
            counter.increment()
            enteredContinuation.yield()
            var gateIterator = gate.makeAsyncIterator()
            _ = await gateIterator.next()
            return Data([0x01])
        }

        let first = Task { await cache.lookup(key: Data([0xBB])) }

        var enteredIterator = fetchEntered.makeAsyncIterator()
        _ = await enteredIterator.next() // the single fetch is now in flight, blocked

        let second = Task { await cache.lookup(key: Data([0xBB])) }
        for _ in 0 ..< 100 {
            await Task.yield()
        } // let `second` reach the coalesced run

        gateContinuation.finish()

        let results = await [first.value, second.value]
        #expect(results == [Data([0x01]), Data([0x01])])
        #expect(counter.count == 1)

        let cached = await cache.lookup(key: Data([0xBB]))
        #expect(cached == Data([0x01]))
        #expect(counter.count == 1)
    }

    @Test func confirmationCancellationResolvesFalse() async {
        let presenter = TrUAPIConfirmationPresenter(
            routerFacade: ProductRoutersFacade.worker()
        )

        let review = UserConfirmationReview.signRaw(
            .legacyAccount(
                request: HostSignRawWithLegacyAccountRequest(signer: "5Ffff", payload: .payload(payload: "hello")),
                watermarked: true
            )
        )
        let task = Task {
            await presenter.confirm(review: review, from: "test.product")
        }
        task.cancel()
        let verdict = await task.value
        #expect(!verdict)
    }

    @Test func actionConfirmationWithoutPresentationDenies() async {
        let presenter = TrUAPIConfirmationPresenter(
            routerFacade: ProductRoutersFacade.worker()
        )

        for review in [
            UserConfirmationReview.preimageSubmit(PreimageSubmitReview(size: 1_024)),
            .productSubtree(ProductSubtreeReview(productId: "test.product"))
        ] {
            #expect(await presenter.confirm(review: review, from: "test.product") == false)
        }
    }

    // MARK: currentTheme

    @Test(arguments: [ThemeSelection.berlinDay, .berlinNight, .lisbon])
    func currentThemeReadsSelectedThemeSynchronously(selection: ThemeSelection) throws {
        let manager = MockThemeManager(selection: selection)
        let bridge = makeBridge(themeManager: manager)
        defer { bridge.detach() }

        // No task suspension or execution attachment is needed for the core's
        // first synchronous read. Lisbon is light despite its light status bar.
        let theme = try bridge.currentTheme()
        #expect(theme.name == .custom(selection.rawValue))
        #expect(theme.variant == (selection == .berlinNight ? .dark : .light))
    }

    @Test(.timeLimit(.minutes(1)))
    func themeTransitionsPreserveOrderIncludingNameOnlyChanges() async throws {
        let manager = MockThemeManager()
        let bridge = makeBridge(themeManager: manager)
        let execution = MockProductExecution()
        bridge.attach(execution)
        defer { bridge.detach() }

        let expected = [
            HostThemeSubscribeItem(name: .custom("berlinDay"), variant: .light),
            HostThemeSubscribeItem(name: .custom("lisbon"), variant: .light),
            HostThemeSubscribeItem(name: .custom("berlinNight"), variant: .dark)
        ]
        await withCheckedContinuation { continuation in
            execution.onThemeChanged = { _ in
                if execution.themeChanges.count == expected.count {
                    continuation.resume()
                }
            }
            // Queue all changes before the observation task runs.
            manager.select(.app(.berlinDay))
            manager.select(.app(.lisbon))
            manager.select(.app(.berlinNight))
        }
        execution.onThemeChanged = nil

        #expect(execution.themeChanges == expected)
        #expect(try bridge.currentTheme() == expected.last)
    }

    @Test(.timeLimit(.minutes(1)))
    func closingExecutionDetachesBeforeCloseAndDiscardsBufferedThemes() async throws {
        let manager = MockThemeManager()
        let pool = MockChainConnections()
        let bridge = makeBridge(themeManager: manager, chainConnections: pool)
        let execution = MockProductExecution()
        bridge.attach(execution)
        let model = RustRuntimeEnvironment.ExecutionModel(
            execution: execution,
            chainConnections: pool,
            osPermissionAsker: MockOSPermissionAsker(),
            bridge: bridge
        )

        await withCheckedContinuation { continuation in
            execution.onThemeChanged = { _ in continuation.resume() }
            manager.select(.app(.berlinDay))
        }
        execution.onThemeChanged = nil
        let delivered = execution.themeChanges
        let cached = try bridge.currentTheme()

        execution.onClose = {
            #expect(pool.eventHandler == nil)
            manager.select(.app(.berlinNight))
        }
        manager.select(.app(.lisbon))
        model.close()
        manager.select(.app(.berlinDay))
        await manager.waitForObservationToEnd()

        #expect(execution.themeChanges == delivered)
        #expect(try bridge.currentTheme() == cached)
        #expect(execution.stopWsBridgeCallCount == 1)
        #expect(execution.closeCallCount == 1)
    }

    @Test(.timeLimit(.minutes(1)))
    func releasingUnattachedBridgeCancelsThemeObservation() async {
        let manager = MockThemeManager()
        var bridge: RustProductExecutionBridge? = makeBridge(themeManager: manager)
        weak var releasedBridge = bridge

        bridge = nil
        await manager.waitForObservationToEnd()

        #expect(releasedBridge == nil)
    }

    // MARK: attach

    /// Not attached: event forwarding is a no-op and must not crash.
    @Test func chainEventForwardingRequiresAttach() {
        let bridge = makeBridge()

        bridge.chainDidReceiveResponse(connectionId: 1, json: "{}")
        bridge.chainDidClose(connectionId: 1)
    }

    /// Plain Swift errors from storage surface as FFI `HostLocalStorageReadError.Unknown`.
    @Test func storageErrorsAreMappedToFfiTypes() {
        let bridge = makeBridge(productStorageFails: true)

        #expect {
            try bridge.storage.read(key: "k")
        } throws: { error in
            guard case HostLocalStorageReadError.Unknown = error else {
                return false
            }
            return true
        }
    }

    /// After `attach`, the bridge sets itself as the chain event handler on
    /// the connection pool. Mocked execution — the Rust cdylib never boots in
    /// unit tests.
    @Test func attachWiresChainEventHandlerOnPool() {
        let chainRegistry = MockChainRegistry()
        let pool = makeRegistryPool(chainRegistry: chainRegistry)
        let bridge = RustProductExecutionBridge(dependencies: .init(
            productId: "test.dot",
            permissionGuard: MockPermissionGuard(),
            osPermissionAsker: MockOSPermissionAsker(),
            notificationScheduler: MockNotificationScheduler(),
            navigationRouter: MockNavigationRouter(),
            chainRegistry: chainRegistry,
            chainConnections: pool,
            productStorage: TrUAPILocalStorage.createProductLocalStorage(
                productId: "test.dot",
                defaults: makeTestDefaults()
            ),
            coreStorage: TrUAPILocalStorage.createCoreLocalStorage(defaults: makeTestDefaults()),
            confirmationPresenter: MockConfirmationPresenter(),
            chatFiles: UnavailableNativeChatFiles(),
            preimageCache: TrUAPIPreimageCache { _ in nil },
            hostProvider: StubHostProvider(),
            themeManager: MockThemeManager(),
            logger: Logger.shared
        ))

        #expect(pool.eventHandler == nil)

        let execution = MockProductExecution()
        bridge.attach(execution)

        #expect(pool.eventHandler === bridge)
    }

    /// After `attach`, chain notify-backs route to the opened execution.
    @Test func chainEventForwardingRoutesToAttachedExecution() {
        let bridge = makeBridge()
        let execution = MockProductExecution()
        bridge.attach(execution)

        bridge.chainDidReceiveResponse(connectionId: 7, json: "{\"ok\":true}")
        bridge.chainDidClose(connectionId: 7)

        #expect(execution.chainResponses.count == 1)
        #expect(execution.chainResponses.first?.0 == 7)
        #expect(execution.chainResponses.first?.1 == "{\"ok\":true}")
        #expect(execution.chainClosed == [7])
    }

    /// Core storage keys (`Data`) are hex-encoded for the underlying store;
    /// a write then read round-trips through the hex key.
    @Test func coreStorageBackendHexEncodesKeys() throws {
        let bridge = makeBridge()
        let key = Data([0xDE, 0xAD])
        let value = Data([0x01, 0x02, 0x03])

        try bridge.coreStorage.write(key: key, value: value)
        let read = try bridge.coreStorage.read(key: key)

        #expect(read == value)
    }
}
