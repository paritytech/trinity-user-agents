import ChainRegistry
import Foundation
import FoundationExt

protocol DashboardServicesCoordinatorMaking<Coordinator> {
    associatedtype Coordinator: DashboardServicesCoordinating

    @MainActor func makeCoordinator() -> Coordinator?
}

struct DashboardServicesBackground {
    let bootstrapper: BackgroundTaskBootstrapping
    let deadline: Duration
}

/// Once no lease is left the coordinator is stopped and dropped, so a stopped instance is never started again.
/// In the background the interactive part waits for the foreground or a call: a BG task launch also connects the scene.
@MainActor
final class DashboardServicesManager<Coordinator: DashboardServicesCoordinating> {
    private let backgroundBootConfig: DashboardServicesBackground
    private let coordinatorFactory: any DashboardServicesCoordinatorMaking<Coordinator>
    private let connectionRetention: BackgroundConnectionRetaining
    private let activity: any ApplicationActivityProviding
    private let logger: LoggerProtocol

    private var coordinator: Coordinator?
    private var generation: UInt64 = 0
    private var leaseCounts: [DashboardServicesDemand: Int] = [:]
    private var backgroundStartTask: Task<Coordinator?, Never>?
    private var interactiveState: InteractiveState?

    init(
        background: DashboardServicesBackground,
        coordinatorFactory: any DashboardServicesCoordinatorMaking<Coordinator>,
        connectionRetention: BackgroundConnectionRetaining,
        activity: any ApplicationActivityProviding,
        logger: LoggerProtocol
    ) {
        backgroundBootConfig = background
        self.coordinatorFactory = coordinatorFactory
        self.connectionRetention = connectionRetention
        self.activity = activity
        self.logger = logger
    }

    var isInteractiveRunning: Bool {
        if case .started = interactiveState { true } else { false }
    }

    func acquireInteractive() -> DashboardServicesLease<Coordinator>? {
        guard let coordinator = existingOrNewCoordinator() else { return nil }

        let isFirstInteractiveLease = leaseCount(for: .interactive) == 0
        let lease = makeLease(.interactive, for: coordinator, connectionToken: nil)

        if isFirstInteractiveLease {
            startInteractiveWhenNeeded(coordinator)
        }

        return lease
    }

    /// An incoming call needs chat signalling even while the app stays in the background.
    func requestInteractiveStart() {
        guard !isInteractiveRunning else { return }

        cancelPendingInteractiveStart()

        guard leaseCount(for: .interactive) > 0, let coordinator else {
            interactiveState = .requested(pendingTask: nil)
            return
        }

        startInteractiveNow(coordinator)
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
        backgroundStartTask?.cancel()
        backgroundStartTask = nil
        cancelPendingInteractiveStart()
        coordinator?.stopAll()
        coordinator = nil
        interactiveState = nil
        leaseCounts = [:]
    }
}

private extension DashboardServicesManager {
    enum InteractiveState {
        /// Without a task: a call asked before any dashboard lease.
        case requested(pendingTask: Task<Void, Never>?)
        case started
    }

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

        if let backgroundStartTask {
            return await backgroundStartTask.value
        }

        let boot = Task { await bootHeadless(generation: generation) }
        backgroundStartTask = boot
        let result = await boot.value
        backgroundStartTask = nil
        return result
    }

    func bootHeadless(generation bootGeneration: UInt64) async -> Coordinator? {
        let readiness = await backgroundBootConfig.bootstrapper.waitWhenReady(deadline: backgroundBootConfig.deadline)

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

        if !hasInteractive {
            cancelPendingInteractiveStart()
        }

        switch (lease.demand, hasInteractive, hasDurability) {
        case (_, false, false):
            coordinator.stopAll()
            self.coordinator = nil
            markInteractiveStopped()
        case (.interactive, false, true):
            coordinator.stopInteractive()
            markInteractiveStopped()
        default:
            break
        }
    }

    func startInteractiveWhenNeeded(_ coordinator: Coordinator) {
        switch interactiveState {
        case .requested(pendingTask: .none):
            startInteractiveNow(coordinator)
        case nil where !activity.isInBackground:
            startInteractiveNow(coordinator)
        case nil:
            interactiveState = .requested(pendingTask: makeForegroundWait())
        case .requested,
             .started:
            break
        }
    }

    func makeForegroundWait() -> Task<Void, Never> {
        let startGeneration = generation

        return Task { [weak self, activity] in
            await activity.waitUntilInForeground()

            guard !Task.isCancelled else { return }

            self?.openInteractiveGate(generation: startGeneration)
        }
    }

    func openInteractiveGate(generation gateGeneration: UInt64) {
        guard
            gateGeneration == generation,
            case .requested = interactiveState,
            leaseCount(for: .interactive) > 0,
            let coordinator
        else {
            return
        }

        startInteractiveNow(coordinator)
    }

    func startInteractiveNow(_ coordinator: Coordinator) {
        interactiveState = .started
        coordinator.startInteractive()
    }

    func cancelPendingInteractiveStart() {
        guard case let .requested(pendingTask?) = interactiveState else { return }

        pendingTask.cancel()
        interactiveState = nil
    }

    /// Keeps a call's request made before any lease.
    func markInteractiveStopped() {
        guard case .started = interactiveState else { return }

        interactiveState = nil
    }
}
