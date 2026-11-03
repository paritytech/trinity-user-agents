import Foundation
import Testing

@testable import polkadot_app

@MainActor
struct DashboardServicesManagerTests {
    @Test func interactiveLeaseCreatesAndStartsCoordinatorOnce() throws {
        let factory = DashboardServicesCoordinatorFactorySpy()
        let manager = makeManager(factory: factory)

        let first = try #require(manager.acquireInteractive())
        let second = try #require(manager.acquireInteractive())

        #expect(first.coordinator === second.coordinator)
        #expect(factory.made.count == 1)
        #expect(first.coordinator.startInteractiveCallCount == 1)
    }

    @Test func durabilityLeaseReusesLiveCoordinatorWithoutBooting() async throws {
        let bootstrapper = BackgroundTaskBootstrapperStub()
        let factory = DashboardServicesCoordinatorFactorySpy()
        let manager = makeManager(bootstrapper: bootstrapper, factory: factory)

        let interactive = try #require(manager.acquireInteractive())
        let durability = try #require(await manager.acquireDurability())

        #expect(durability.coordinator === interactive.coordinator)
        #expect(bootstrapper.waitCallCount == 0)
        #expect(durability.coordinator.startDurabilityCallCount == 1)
    }

    @Test func coldDurabilityLeaseBootsHeadlessAndStartsOnlyDurability() async throws {
        let bootstrapper = BackgroundTaskBootstrapperStub(readiness: .dashboard)
        let factory = DashboardServicesCoordinatorFactorySpy()
        let manager = makeManager(bootstrapper: bootstrapper, factory: factory)

        let lease = try #require(await manager.acquireDurability())

        #expect(bootstrapper.waitCallCount == 1)
        #expect(factory.made.count == 1)
        #expect(lease.coordinator.startDurabilityCallCount == 1)
        #expect(lease.coordinator.startInteractiveCallCount == 0)
    }

    @Test func coldDurabilityLeaseIsRefusedWhenNotReady() async {
        let factory = DashboardServicesCoordinatorFactorySpy()
        let manager = makeManager(
            bootstrapper: BackgroundTaskBootstrapperStub(readiness: .notOnboarded),
            factory: factory
        )

        let lease = await manager.acquireDurability()

        #expect(lease == nil)
        #expect(factory.made.isEmpty)
    }

    @Test func concurrentColdDurabilityLeasesBootOnce() async throws {
        let bootstrapper = BackgroundTaskBootstrapperStub(holdsReadiness: true)
        let factory = DashboardServicesCoordinatorFactorySpy()
        let manager = makeManager(bootstrapper: bootstrapper, factory: factory)

        async let first = manager.acquireDurability()
        async let second = manager.acquireDurability()
        await waitUntil { bootstrapper.waitCallCount == 1 }
        bootstrapper.releaseReadiness()

        let leases = await [first, second]

        #expect(bootstrapper.waitCallCount == 1)
        #expect(factory.made.count == 1)
        #expect(leases.compactMap { $0 }.count == 2)
    }

    @Test func dashboardCreatedDuringHeadlessBootIsReused() async throws {
        let bootstrapper = BackgroundTaskBootstrapperStub(holdsReadiness: true)
        let factory = DashboardServicesCoordinatorFactorySpy()
        let manager = makeManager(bootstrapper: bootstrapper, factory: factory)

        async let durability = manager.acquireDurability()
        await waitUntil { bootstrapper.waitCallCount == 1 }
        let interactive = try #require(manager.acquireInteractive())
        bootstrapper.releaseReadiness()

        let durabilityLease = try #require(await durability)

        #expect(durabilityLease.coordinator === interactive.coordinator)
        #expect(factory.made.count == 1)
    }

    @Test func releasingInteractiveDuringBackgroundRunStopsOnlyInteractivePart() async throws {
        let manager = makeManager()
        let interactive = try #require(manager.acquireInteractive())
        let durability = try #require(await manager.acquireDurability())
        let coordinator = interactive.coordinator

        interactive.release()
        await waitUntil { coordinator.stopInteractiveCallCount == 1 }

        #expect(coordinator.stopAllCallCount == 0)
        withExtendedLifetime(durability) {}
    }

    @Test func releasingLastLeaseStopsAllAndNextLeaseBuildsFreshCoordinator() async throws {
        let factory = DashboardServicesCoordinatorFactorySpy()
        let manager = makeManager(factory: factory)
        let lease = try #require(await manager.acquireDurability())
        let coordinator = lease.coordinator

        lease.release()
        await waitUntil { coordinator.stopAllCallCount == 1 }
        let next = try #require(manager.acquireInteractive())

        #expect(next.coordinator !== coordinator)
        #expect(factory.made.count == 2)
    }

    @Test func releaseAfterResetIsIgnored() async throws {
        let manager = makeManager()
        let lease = try #require(manager.acquireInteractive())
        let coordinator = lease.coordinator

        manager.reset()
        lease.release()
        await settle()

        #expect(coordinator.stopAllCallCount == 1)
        #expect(coordinator.stopInteractiveCallCount == 0)
    }

    @Test func durabilityLeaseKeepsConnectionsAwakeUntilReleased() async throws {
        let retention = ConnectionRetentionSpy()
        let manager = makeManager(connectionRetention: retention)

        let lease = try #require(await manager.acquireDurability())
        #expect(retention.heldCount == 1)

        lease.release()

        #expect(retention.retainCount == 1)
        #expect(retention.heldCount == 0)
    }

    @Test func interactiveLeaseLeavesConnectionsToTheForegroundApp() throws {
        let retention = ConnectionRetentionSpy()
        let manager = makeManager(connectionRetention: retention)

        _ = try #require(manager.acquireInteractive())

        #expect(retention.retainCount == 0)
    }

    @Test func failedDurabilityStartReleasesConnections() async {
        let retention = ConnectionRetentionSpy()
        let manager = makeManager(
            factory: DashboardServicesCoordinatorFactorySpy(durabilityStarts: false),
            connectionRetention: retention
        )

        _ = await manager.acquireDurability()

        #expect(retention.retainCount == 1)
        #expect(retention.heldCount == 0)
    }

    @Test func failedDurabilityStartDropsCoordinator() async {
        let factory = DashboardServicesCoordinatorFactorySpy(durabilityStarts: false)
        let manager = makeManager(factory: factory)

        let lease = await manager.acquireDurability()
        let coordinator = factory.made.first
        await waitUntil { coordinator?.stopAllCallCount == 1 }

        #expect(lease == nil)
        #expect(coordinator?.stopAllCallCount == 1)
    }
}

private extension DashboardServicesManagerTests {
    func makeManager(
        bootstrapper: BackgroundTaskBootstrapperStub = BackgroundTaskBootstrapperStub(),
        factory: DashboardServicesCoordinatorFactorySpy? = nil,
        connectionRetention: ConnectionRetentionSpy = ConnectionRetentionSpy()
    ) -> DashboardServicesManager<DashboardServicesCoordinatorSpy> {
        DashboardServicesManager(
            bootstrapper: bootstrapper,
            coordinatorFactory: factory ?? DashboardServicesCoordinatorFactorySpy(),
            connectionRetention: connectionRetention,
            headlessBootDeadline: .seconds(1),
            logger: StubLogger()
        )
    }

    /// Lease releases hop to the main actor; give them a bounded number of turns to land.
    func waitUntil(_ condition: () -> Bool) async {
        var turns = 0

        while !condition(), turns < 1_000 {
            await Task.yield()
            turns += 1
        }
    }

    func settle() async {
        for _ in 0 ..< 100 {
            await Task.yield()
        }
    }
}
