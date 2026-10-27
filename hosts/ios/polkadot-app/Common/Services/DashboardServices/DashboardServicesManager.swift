import ChainRegistry
import Foundation

protocol DashboardServicesCoordinatorMaking<Coordinator> {
    associatedtype Coordinator: DashboardServicesCoordinating

    @MainActor func makeCoordinator() -> Coordinator?
}

/// Once no lease is left the coordinator is stopped and dropped, so a stopped instance is never started again.
@MainActor
final class DashboardServicesManager<Coordinator: DashboardServicesCoordinating> {
    private let bootstrapper: BackgroundTaskBootstrapping
    private let coordinatorFactory: any DashboardServicesCoordinatorMaking<Coordinator>
    private let connectionRetention: BackgroundConnectionRetaining
    private let headlessBootDeadline: Duration
    private let logger: LoggerProtocol

    private var coordinator: Coordinator?
    private var generation: UInt64 = 0
    private var leaseCounts: [DashboardServicesDemand: Int] = [:]
    private var headlessBoot: Task<Coordinator?, Never>?

    init(
        bootstrapper: BackgroundTaskBootstrapping,
        coordinatorFactory: any DashboardServicesCoordinatorMaking<Coordinator>,
        connectionRetention: BackgroundConnectionRetaining,
        headlessBootDeadline: Duration,
        logger: LoggerProtocol
    ) {
        self.bootstrapper = bootstrapper
        self.coordinatorFactory = coordinatorFactory
        self.connectionRetention = connectionRetention
        self.headlessBootDeadline = headlessBootDeadline
        self.logger = logger
    }

    var hasCoordinator: Bool {
        coordinator != nil
    }

    func acquireInteractive() -> DashboardServicesLease<Coordinator>? {
        guard let coordinator = existingOrNewCoordinator() else { return nil }

        if leaseCount(for: .interactive) == 0 {
            coordinator.startInteractive()
        }

        return makeLease(.interactive, for: coordinator, connectionToken: nil)
    }

    func acquireDurability() async -> DashboardServicesLease<Coordinator>? {
        guard let coordinator = await existingOrHeadlessCoordinator() else { return nil }

        // After the boot: reaching the registry opens the chain store, which migrations must precede.
        let connectionToken = connectionRetention.retainAllConnections()
        let lease = makeLease(.durability, for: coordinator, connectionToken: connectionToken)

        guard await coordinator.startDurability() else {
            lease.release()
            return nil
        }

        return lease
    }

    func reset() {
        generation &+= 1
        headlessBoot?.cancel()
        headlessBoot = nil
        coordinator?.stopAll()
        coordinator = nil
        leaseCounts = [:]
    }
}

private extension DashboardServicesManager {
    func existingOrNewCoordinator() -> Coordinator? {
        if let coordinator {
            return coordinator
        }

        coordinator = coordinatorFactory.makeCoordinator()
        return coordinator
    }

    func existingOrHeadlessCoordinator() async -> Coordinator? {
        if let coordinator {
            return coordinator
        }

        if let headlessBoot {
            return await headlessBoot.value
        }

        let boot = Task { await bootHeadless(generation: generation) }
        headlessBoot = boot
        let result = await boot.value
        headlessBoot = nil
        return result
    }

    func bootHeadless(generation bootGeneration: UInt64) async -> Coordinator? {
        let readiness = await bootstrapper.waitWhenReady(deadline: headlessBootDeadline)

        guard readiness == .dashboard else {
            logger.info("Headless boot skipped: \(readiness)")
            return nil
        }

        guard bootGeneration == generation, !Task.isCancelled else { return nil }

        // The dashboard may have created it while the boot waited.
        return existingOrNewCoordinator()
    }

    func leaseCount(for demand: DashboardServicesDemand) -> Int {
        leaseCounts[demand, default: 0]
    }

    func makeLease(
        _ demand: DashboardServicesDemand,
        for coordinator: Coordinator,
        connectionToken: (any ConnectionTokenProtocol)?
    ) -> DashboardServicesLease<Coordinator> {
        leaseCounts[demand, default: 0] += 1

        return DashboardServicesLease(
            coordinator: coordinator,
            demand: demand,
            generation: generation,
            connectionToken: connectionToken
        ) { [weak self] in
            self?.didRelease($0)
        }
    }

    func didRelease(_ lease: DashboardServicesLease<Coordinator>) {
        guard
            lease.generation == generation,
            let coordinator,
            coordinator === lease.coordinator
        else {
            return
        }

        leaseCounts[lease.demand] = max(leaseCount(for: lease.demand) - 1, 0)

        let hasInteractive = leaseCount(for: .interactive) > 0
        let hasDurability = leaseCount(for: .durability) > 0

        switch (lease.demand, hasInteractive, hasDurability) {
        case (_, false, false):
            coordinator.stopAll()
            self.coordinator = nil
        case (.interactive, false, true):
            coordinator.stopInteractive()
        default:
            break
        }
    }
}
