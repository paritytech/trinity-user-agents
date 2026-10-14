import Foundation
import Testing
import ChainRegistry
import Products
import Clocks

@testable import polkadot_app

@Suite("Root interactor setup and retry")
struct RootInteractorSetupTests {
    @Test("migrations run exactly once across setup and retry")
    @MainActor
    func migrationsRunOnceAcrossSetupAndRetry() {
        let migrator = MockMigrator()
        let interactor = makeInteractor(migrator: migrator)

        interactor.setup()
        #expect(migrator.migrateCallCount == 1)

        interactor.retrySetup()
        #expect(migrator.migrateCallCount == 1)
    }

    @Test("TLD resolution failure gates destination")
    @MainActor
    func tldFailureGatesDestination() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        chainRegistry.chainsOnSubscribe = [
            makeChain(id: AppConfig.Chains.usernameChain),
            makeChain(id: AppConfig.Chains.bulletInChain),
            makeChain(id: AppConfig.Chains.assethubChain)
        ]

        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            tldProvider: StubDotNsTldProvider(tld: nil)
        )
        interactor.presenter = spy

        interactor.setup()

        let kind = await spy.nextFailureKind()

        #expect(kind == .configuration(.tld), "Expected the configuration failure at the tld stage")
        #expect(spy.didFailSetupCallCount == 1, "Expected one setup failure to be reported")
        #expect(spy.didDecideCallCount == 0, "Expected no destination decision (gate holds)")
    }

    @Test("a hanging config does not fail at the setup deadline")
    @MainActor
    func hangingConfigDoesNotFailAtSetupDeadline() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        chainRegistry.chainsOnSubscribe = [
            makeChain(id: AppConfig.Chains.usernameChain),
            makeChain(id: AppConfig.Chains.bulletInChain),
            makeChain(id: AppConfig.Chains.assethubChain)
        ]
        let remoteConfigManager = MockRemoteConfigManager()
        remoteConfigManager.hangs = true
        let pathMonitor = MockNetworkPathMonitor()
        let clock = TestClock<Duration>()

        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            pathMonitor: pathMonitor,
            remoteConfigManager: remoteConfigManager,
            clock: clock
        )
        interactor.presenter = spy

        interactor.setup()

        await clock.advance(by: .seconds(11))

        for _ in 0 ..< 10 {
            await Task.yield()
        }

        pathMonitor.send(false)

        let kind = await spy.nextFailureKind()

        #expect(kind == .connectivity, "Expected connectivity failure, not the deadline")
        #expect(spy.didFailSetupCallCount == 1)
        #expect(spy.didDecideCallCount == 0)
    }

    @Test("offline during the config wait fails at the offline deadline")
    @MainActor
    func offlineDuringConfigWaitFailsAtOfflineDeadline() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        let remoteConfigManager = MockRemoteConfigManager()
        remoteConfigManager.hangs = true
        let pathMonitor = MockNetworkPathMonitor(initial: false)
        let clock = TestClock<Duration>()

        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            pathMonitor: pathMonitor,
            remoteConfigManager: remoteConfigManager,
            clock: clock
        )
        interactor.presenter = spy

        interactor.setup()

        await clock.advance(by: .seconds(4))

        let kind = await spy.nextFailureKind()

        #expect(kind == .connectivity)
    }

    @Test("invalid config fails at the config stage even without chains")
    @MainActor
    func invalidConfigFailsAtConfigStageEvenWithoutChains() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        let remoteConfigManager = MockRemoteConfigManager()
        remoteConfigManager.errorToThrow = RemoteConfigError.invalidConfig
        let clock = TestClock<Duration>()

        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            remoteConfigManager: remoteConfigManager,
            clock: clock
        )
        interactor.presenter = spy

        interactor.setup()

        let kind = await spy.nextFailureKind()

        #expect(kind == .configuration(.config))
        #expect(chainRegistry.chainsUnsubscribeCallCount == 0)
    }

    @Test("unsatisfied path outranks a TLD failure")
    @MainActor
    func unsatisfiedPathOutranksTldFailure() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        chainRegistry.chainsOnSubscribe = [
            makeChain(id: AppConfig.Chains.usernameChain),
            makeChain(id: AppConfig.Chains.bulletInChain),
            makeChain(id: AppConfig.Chains.assethubChain)
        ]

        let pathMonitor = MockNetworkPathMonitor(initial: false)
        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            pathMonitor: pathMonitor,
            tldProvider: StubDotNsTldProvider(tld: nil)
        )
        interactor.presenter = spy

        interactor.setup()

        let kind = await spy.nextFailureKind()

        #expect(kind == .connectivity, "Expected connectivity failure instead of TLD failure")
    }

    @Test("path recovery reports connectivity recovery")
    @MainActor
    func pathRecoveryReportsConnectivityRecovery() async throws {
        let spy = RootSetupOutputSpy()
        let migrator = MockMigrator()
        let chainRegistry = MockChainRegistry()
        let pathMonitor = MockNetworkPathMonitor(initial: false)
        let clock = TestClock<Duration>()

        let interactor = makeInteractor(
            migrator: migrator,
            chainRegistry: chainRegistry,
            pathMonitor: pathMonitor,
            clock: clock
        )
        interactor.presenter = spy

        interactor.setup()

        // Advance the clock to trigger the offline deadline failure
        await clock.advance(by: .seconds(4))

        let kind = await spy.nextFailureKind()
        #expect(kind == .connectivity, "Expected connectivity failure")

        pathMonitor.send(true)

        await spy.nextRecovery()

        #expect(spy.didRecoverConnectivityCallCount == 1, "Expected one connectivity recovery report")
        #expect(migrator.migrateCallCount == 1, "Expected migrations to stay on the launch pass")
    }

    @Test("a path drop during setup reports connectivity immediately")
    @MainActor
    func pathDropDuringSetupReportsConnectivityImmediately() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        chainRegistry.chainsOnSubscribe = [
            makeChain(id: AppConfig.Chains.usernameChain),
            makeChain(id: AppConfig.Chains.bulletInChain),
            makeChain(id: AppConfig.Chains.assethubChain)
        ]

        let pathMonitor = MockNetworkPathMonitor()
        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            pathMonitor: pathMonitor,
            tldProvider: StubDotNsTldProvider(tld: nil)
        )
        interactor.presenter = spy

        interactor.setup()

        pathMonitor.send(false)

        let kind = await spy.nextFailureKind()

        #expect(kind == .connectivity, "Expected a path drop to report connectivity")
    }

    @Test("cold offline launch fails fast")
    @MainActor
    func coldOfflineLaunchFailsFast() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        let pathMonitor = MockNetworkPathMonitor(initial: false)
        let clock = TestClock<Duration>()

        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            pathMonitor: pathMonitor,
            clock: clock
        )
        interactor.presenter = spy

        interactor.setup()

        // Advance the clock past the offline deadline (3s) to trigger offline path failure
        await clock.advance(by: .seconds(4))

        let kind = await spy.nextFailureKind()

        #expect(kind == .connectivity, "Expected connectivity failure when offline")
        #expect(
            chainRegistry.chainsUnsubscribeCallCount == 1,
            "Expected the pending chain wait to be cancelled at the offline deadline"
        )
    }

    @Test("warm offline launch still succeeds")
    @MainActor
    func warmOfflineLaunchStillSucceeds() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        chainRegistry.chainsOnSubscribe = [
            makeChain(id: AppConfig.Chains.usernameChain),
            makeChain(id: AppConfig.Chains.bulletInChain),
            makeChain(id: AppConfig.Chains.assethubChain)
        ]
        let pathMonitor = MockNetworkPathMonitor(initial: false)
        let clock = TestClock<Duration>()

        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            pathMonitor: pathMonitor,
            clock: clock
        )
        interactor.presenter = spy

        interactor.setup()

        await spy.nextDecision()

        #expect(
            spy.didDecideCallCount == 1,
            "Expected a warm offline launch to reach a destination"
        )

        await clock.advance(by: .seconds(4))

        #expect(
            spy.didFailSetupCallCount == 0,
            "Expected the deadline to not fire after a warm launch has already succeeded"
        )
    }

    @Test("invalid remote config fails at the config stage")
    @MainActor
    func invalidRemoteConfigFailsAtConfigStage() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        chainRegistry.chainsOnSubscribe = [
            makeChain(id: AppConfig.Chains.usernameChain),
            makeChain(id: AppConfig.Chains.bulletInChain),
            makeChain(id: AppConfig.Chains.assethubChain)
        ]
        let remoteConfigManager = MockRemoteConfigManager()
        remoteConfigManager.errorToThrow = RemoteConfigError.invalidConfig

        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            remoteConfigManager: remoteConfigManager
        )
        interactor.presenter = spy

        interactor.setup()

        let kind = await spy.nextFailureKind()

        #expect(
            kind == .configuration(.config),
            "Expected a configuration failure at the config stage"
        )
    }

    @Test("a deadline with required chains missing fails at the chains stage")
    @MainActor
    func deadlineWithMissingChainsFails() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        // No chains emitted; the subscription never resolves
        let clock = TestClock<Duration>()

        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            clock: clock
        )
        interactor.presenter = spy

        interactor.setup()

        // Advance the clock past the full deadline (10s) to trigger timeout
        await clock.advance(by: .seconds(11))

        let kind = await spy.nextFailureKind()

        #expect(
            kind == .configuration(.chains),
            "Expected a configuration failure at the chains stage when required chains are missing"
        )
        #expect(
            chainRegistry.chainsUnsubscribeCallCount == 1,
            "Expected the pending chain wait to be cancelled at the deadline"
        )
    }

    @Test("retry does not start a second signal consumer")
    @MainActor
    func retryDoesNotStartSecondSignalConsumer() async throws {
        let spy = RootSetupOutputSpy()
        let pathMonitor = MockNetworkPathMonitor()
        let chainRegistry = MockChainRegistry()

        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            pathMonitor: pathMonitor
        )
        interactor.presenter = spy

        interactor.setup()
        interactor.retrySetup()

        pathMonitor.send(false)

        let kind = await spy.nextFailureKind()

        #expect(
            spy.didFailSetupCallCount == 1,
            "Expected exactly one failure from a single signal"
        )
        #expect(
            kind == .connectivity,
            "Expected connectivity failure from the path drop"
        )
    }

    @Test("setup fails at the config stage when no config was applied")
    @MainActor
    func setupFailsWhenNoConfigApplied() async throws {
        let spy = RootSetupOutputSpy()
        let chainRegistry = MockChainRegistry()
        chainRegistry.chainsOnSubscribe = [
            makeChain(id: AppConfig.Chains.usernameChain),
            makeChain(id: AppConfig.Chains.bulletInChain),
            makeChain(id: AppConfig.Chains.assethubChain)
        ]

        let interactor = makeInteractor(
            chainRegistry: chainRegistry,
            appliedConfigReader: { nil }
        )
        interactor.presenter = spy

        interactor.setup()

        let kind = await spy.nextFailureKind()

        #expect(
            kind == .configuration(.config),
            "Expected configuration failure when no config was applied"
        )
    }
}

private extension RootInteractorSetupTests {
    @MainActor
    func makeInteractor(
        migrator: Migrating = MockMigrator(),
        chainRegistry: MockChainRegistry = MockChainRegistry(),
        pathMonitor: NetworkPathMonitoring = MockNetworkPathMonitor(),
        remoteConfigManager: MockRemoteConfigManager = MockRemoteConfigManager(),
        tldProvider: DotNsTldProviding = StubDotNsTldProvider(tld: "dot"),
        appliedConfigReader: @escaping () -> RemoteAppConfig? = {
            RemoteAppConfig(
                identityBackendUrl: URL(string: "https://example.com"),
                ipfsGatewayUrl: URL(string: "https://ipfs.example.com"),
                dotNsResolver: "resolver.example.com",
                dotNsNameRegistry: nil,
                coinageInstanceId: 1,
                fundingUrl: nil,
                offrampUrl: nil,
                accountDataStoreContract: nil,
                paymentAsset: nil,
                appSharingUrl: nil
            )
        },
        clock: any Clock<Duration> = TestClock<Duration>()
    ) -> RootInteractor {
        RootInteractor(
            chainRegistryClosure: { chainRegistry },
            migrator: migrator,
            logger: StubLogger(),
            resolver: MockDecisionResolver(),
            tokenManager: MockJWTTokenManager(),
            remoteConfigManager: remoteConfigManager,
            chainRegistryConfigurator: MockChainRegistryConfigurator(),
            productPrewarmer: MockProductContentPrewarmer(),
            observer: RootSetupObserver(pathMonitor: pathMonitor),
            tldProvider: tldProvider,
            appliedConfigReader: appliedConfigReader,
            clock: clock
        )
    }

    func makeChain(id: String) -> ChainModel {
        ChainModel(
            chainId: id,
            parentId: nil,
            name: id,
            assets: [],
            nodes: [],
            nodeSwitchStrategy: .roundRobin,
            addressPrefix: 0,
            explicitGenesisHash: nil,
            types: nil,
            icon: nil,
            options: nil,
            externalApis: nil,
            explorers: nil,
            order: 0,
            additional: nil,
            syncMode: .full
        )
    }
}
