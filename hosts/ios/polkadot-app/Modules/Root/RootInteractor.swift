import Foundation
import os
import NovaCrypto
import Operation_iOS
import Foundation_iOS
import SubstrateSdk
import ChainRegistry
import SubstrateSdkExt
import Products
import StructuredConcurrency

final class RootInteractor {
    private enum SetupWaitOutcome {
        case ready
        case deadlineExpired
        case chainsIncomplete
        case configurationBroken

        /// The failure to report for this outcome, or nil when setup may continue.
        var failureFallback: RootSetupFailureKind? {
            switch self {
            case .ready: nil
            case .deadlineExpired: .unknown
            case .chainsIncomplete: .configuration(.chains)
            case .configurationBroken: .configuration(.config)
            }
        }
    }

    private struct SetupDeadlineExpired: Error {}

    private enum Constants {
        static let setupDeadlineSeconds: TimeInterval = 10
        /// Applied when the path is already unsatisfied: locally-served chains and a cached config
        /// still resolve well inside it, so only a genuinely blocked wait is cut short.
        static let offlineSetupDeadlineSeconds: TimeInterval = 3
        /// TLD resolution waits on the contracts chain's runtime metadata sync (~1.25 MB on the wire,
        /// roughly 27s on 3G and 85s on EDGE). The 30s single-attempt timeout and 3-attempt budget
        /// allow ~93 seconds total, preventing failures on slow but working connections. Concurrent
        /// TLD reads are coalesced, so retries only matter after a genuinely failed read.
        static let tldTimeoutSeconds: TimeInterval = 30
        static let tldRetryMaxAttempts = 3
        static let tldRetryInitialDelay: Duration = .seconds(1)
    }

    private static let requiredChainIds: Set<ChainModel.Id> = [
        AppConfig.Chains.usernameChain,
        AppConfig.Chains.bulletInChain,
        AppConfig.Chains.assethubChain
    ]

    weak var presenter: RootInteractorOutputProtocol?

    let chainRegistryClosure: ChainRegistryLazyClosure

    let migrator: Migrating
    let logger: LoggerProtocol
    let resolver: any DecisionResolver<RootDestination>
    let tokenManager: JWTTokenManaging
    let tldProvider: DotNsTldProviding

    let remoteConfigManager: RemoteConfigManaging
    let chainRegistryConfigurator: ChainRegistryConfiguring
    let productPrewarmer: ProductContentPrewarming
    let observer: RootSetupObserver
    let appliedConfigReader: () -> RemoteAppConfig?
    let paymentAssetBranding: PaymentAssetBranding
    let clock: any Clock<Duration>

    private var completionTask: Task<Void, Never>?
    private var didReportEstablishedUser = false
    private var observationTask: Task<Void, Never>?
    private let didReportOutcome = OSAllocatedUnfairLock(initialState: false)

    #if TESTNET_FEATURE
        var appFactoryResetCheckerFactory: AppFactoryResetCheckerFactoryProtocol?
        private var appFactoryResetChecker: AppFactoryResetChecker?
    #endif

    init(
        chainRegistryClosure: @escaping ChainRegistryLazyClosure,
        migrator: Migrating,
        logger: LoggerProtocol,
        resolver: any DecisionResolver<RootDestination>,
        tokenManager: JWTTokenManaging,
        remoteConfigManager: RemoteConfigManaging,
        chainRegistryConfigurator: ChainRegistryConfiguring,
        productPrewarmer: ProductContentPrewarming,
        observer: RootSetupObserver,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        appliedConfigReader: @escaping () -> RemoteAppConfig? = { AppConfigProvider.shared.getRemoteConfig() },
        paymentAssetBranding: PaymentAssetBranding = .shared,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.chainRegistryClosure = chainRegistryClosure

        self.migrator = migrator
        self.logger = logger
        self.resolver = resolver
        self.tokenManager = tokenManager
        self.remoteConfigManager = remoteConfigManager
        self.chainRegistryConfigurator = chainRegistryConfigurator
        self.productPrewarmer = productPrewarmer
        self.observer = observer
        self.tldProvider = tldProvider
        self.appliedConfigReader = appliedConfigReader
        self.paymentAssetBranding = paymentAssetBranding
        self.clock = clock
    }

    deinit {
        completionTask?.cancel()
        observationTask?.cancel()
    }

    @MainActor
    private func performCommonSetup(with chainRegistry: ChainRegistryProtocol) {
        completionTask?.cancel()
        didReportOutcome.withLock { $0 = false }

        setupChainUpdate(for: chainRegistry)
        fetchRemoteConfig()
        startObservationIfNeeded()

        startSetupCompletionTask(for: chainRegistry)
    }

    private func setupChainUpdate(for registry: ChainRegistryProtocol) {
        chainRegistryConfigurator.set(chainRegistry: registry)
    }

    private func runMigrators() {
        do {
            try migrator.migrate()
        } catch {
            logger.error("Migration failed: \(error.localizedDescription)")
        }
    }

    private func fetchRemoteConfig() {
        remoteConfigManager.fetchRemoteConfigValues()

        // Follows every applied config, so the logos are fetched the moment one names them.
        paymentAssetBranding.start()
    }

    private func setupJWTManager() {
        let authProvider = AppAttestProviderResolver.resolve()

        tokenManager.setup(authProvider: authProvider)
        tokenManager.prewarm()
    }

    @MainActor
    private func prewarmProducts(for destination: RootDestination) {
        switch destination {
        case .dashboard:
            productPrewarmer.prewarm()
        default:
            break
        }
    }

    private func startSetupCompletionTask(for chainRegistry: ChainRegistryProtocol) {
        completionTask = Task { [weak self] in
            await self?.performSetupCompletion(for: chainRegistry)
        }
    }

    private func performSetupCompletion(for chainRegistry: ChainRegistryProtocol) async {
        let outcome = await waitForSetupInputs(for: chainRegistry)

        guard !Task.isCancelled else { return }

        if let fallback = outcome.failureFallback {
            await reportFailure(fallback: fallback)
            return
        }

        setupJWTManager()

        do {
            try await resolveTldIfNeeded()
        } catch {
            await reportFailure(fallback: .configuration(.tld))
            return
        }

        guard !Task.isCancelled, claimOutcome() else { return }

        await completeSetup()
    }

    /// Caches the DotNs TLD once chains and remote config are ready. Resolving here covers
    /// every onboarding path (username claim, iCloud recovery), so downstream built-in
    /// account derivation can read the TLD synchronously. A TLD persisted by a previous
    /// run is enough, so startup is not blocked offline; currentTld() kicks a background
    /// refresh on its own.
    private func resolveTldIfNeeded() async throws {
        guard tldProvider.currentTld() == nil else { return }

        _ = try await withRetry(
            maxAttempts: Constants.tldRetryMaxAttempts,
            initialDelay: Constants.tldRetryInitialDelay
        ) { [tldProvider, clock] in
            try await withTimeout(.seconds(Constants.tldTimeoutSeconds), clock: clock) {
                try await tldProvider.resolveTld()
            }
        }
    }

    /// Waits for remote config first, bounded only by the offline deadline, so a fetch still retrying never
    /// times out and an invalid config fails at once; then waits for the required chains under the full deadline.
    private func waitForSetupInputs(for chainRegistry: ChainRegistryProtocol) async -> SetupWaitOutcome {
        do {
            try await waitForRemoteConfig()
            try await waitForChains(chainRegistry)

            // The wait tolerates non-invalidity errors, so reaching this point does not by itself
            // prove a config was applied. If no valid config landed, the force-unwrapping accessors
            // would trap, so verify the invariant here before setup continues.
            guard appliedConfigReader()?.isValid == true else {
                return .configurationBroken
            }

            return .ready
        } catch RemoteConfigError.invalidConfig {
            return .configurationBroken
        } catch {
            // The deadline expired. The wait cannot tell "not yet" from "not coming", so ask
            // the registry: if required chains are missing, report that; otherwise timeout.
            let availableChainIds = chainRegistry.availableChainIds ?? []

            guard Self.requiredChainIds.isSubset(of: availableChainIds) else {
                return .chainsIncomplete
            }

            return .deadlineExpired
        }
    }

    @MainActor
    private func completeSetup() {
        reevaluate()
    }

    @MainActor
    private func reportSetupFailure(kind: RootSetupFailureKind) {
        presenter?.didFailSetup(kind: kind)
    }

    /// Reports a failure unless this attempt already reported an outcome, or was cancelled.
    @MainActor
    private func reportFailure(fallback: RootSetupFailureKind) {
        guard !Task.isCancelled, claimOutcome() else { return }

        reportSetupFailure(kind: classifyFailure(fallback: fallback))
    }

    @MainActor
    private func handle(_ signal: RootSetupSignal) {
        switch signal {
        case .connectivityRecovered:
            presenter?.didRecoverConnectivity()
        case .connectivityLost:
            abandonSetup(fallback: .connectivity)
        }
    }

    /// Stops the attempt in flight and reports, unless it already reported an outcome.
    @MainActor
    private func abandonSetup(fallback: RootSetupFailureKind) {
        guard claimOutcome() else { return }

        completionTask?.cancel()
        reportSetupFailure(kind: classifyFailure(fallback: fallback))
    }
}

extension RootInteractor: RootInteractorInputProtocol {
    func reevaluate() {
        let destination = (try? resolver.resolve()) ?? .broken
        handleEstablishedUserIfNeeded(for: destination)
        prewarmProducts(for: destination)
        presenter?.didDecide(destination: destination)
    }

    func setup() {
        runMigrators()
        let chainRegistry = chainRegistryClosure()
        performCommonSetup(with: chainRegistry)

        #if TESTNET_FEATURE
            appFactoryResetChecker = appFactoryResetCheckerFactory?
                .makeChecker(chainRegistry: chainRegistry)
        #endif
    }

    func retrySetup() {
        let chainRegistry = chainRegistryClosure()
        performCommonSetup(with: chainRegistry)
    }

    func completeWalletsCreation() {
        reevaluate()
    }

    func completeWalletsRecovery() {
        reevaluate()
    }
}

private extension RootInteractor {
    func handleEstablishedUserIfNeeded(for destination: RootDestination) {
        guard destination.impliesEstablishedUser, !didReportEstablishedUser else {
            return
        }

        didReportEstablishedUser = true
        logWallets()

        #if TESTNET_FEATURE
            scheduleFactoryResetCheck()
        #endif
    }
}

#if TESTNET_FEATURE
    private extension RootInteractor {
        func scheduleFactoryResetCheck() {
            appFactoryResetChecker?.checkIfResetNeeded { [logger, presenter] resetNeeded in
                Task { @MainActor in
                    guard resetNeeded else { return }
                    guard let presenter else {
                        logger.error("Failed to present app reset alert")
                        return
                    }
                    presenter.didRequireAppFactoryReset()
                }
            }
        }
    }
#endif

private extension RootInteractor {
    func logWallets() {
        let walletRepo: WalletManagerRepositoryProtocol = .shared
        let main = try? walletRepo.main().getRawPublicKey().toAddress(using: .genericFormat)
        let resources = try? walletRepo.resourcesAlias().getRawPublicKey().toAddress(using: .genericFormat)

        logger.debug("Main address: \(main ?? "")")
        logger.debug("Resources address: \(resources ?? "")")
    }
}

private extension RootInteractor {
    /// Setup can now finish from two places — the wait and the chain-sync event — so the first one
    /// to report wins and the other is dropped.
    func claimOutcome() -> Bool {
        didReportOutcome.withLock { reported in
            guard !reported else { return false }
            reported = true
            return true
        }
    }

    /// The observer and its signal consumer are per-app-lifetime, unlike completionTask which is per-attempt.
    func startObservationIfNeeded() {
        guard observationTask == nil else {
            return
        }

        observer.start()

        let signals = observer.signals
        observationTask = Task { [weak self] in
            for await signal in signals {
                guard let self else { return }
                await handle(signal)
            }
        }
    }

    /// Connectivity outranks every other cause: an unsatisfied path is the only thing the user can act on.
    func classifyFailure(fallback kind: RootSetupFailureKind) -> RootSetupFailureKind {
        observer.claimConnectivityFailure() ? .connectivity : kind
    }

    /// Ends the wait early when the path is unsatisfied at the offline deadline — by then the monitor has
    /// emitted, so the choice is made on a known value rather than a race with the first emission.
    func enforceSetupDeadline() async throws {
        try await clock.sleep(for: .seconds(Constants.offlineSetupDeadlineSeconds))

        if observer.isPathSatisfied {
            let remaining = Constants.setupDeadlineSeconds - Constants.offlineSetupDeadlineSeconds
            try await clock.sleep(for: .seconds(remaining))
        }

        throw SetupDeadlineExpired()
    }

    func waitForRemoteConfig() async throws {
        try await withThrowingTaskGroup(of: Bool.self) { group in
            group.addTask { [remoteConfigManager] in
                do {
                    _ = try await remoteConfigManager.asyncWaitRemoteConfig()
                } catch RemoteConfigError.invalidConfig {
                    throw RemoteConfigError.invalidConfig
                } catch {
                    // Other config errors stay non-fatal on their own
                }
                return true
            }

            group.addTask { [weak self] in
                guard let self else { return false }
                try await enforceOfflineDeadline()
                return false
            }

            while let isConfigSettled = try await group.next() {
                if isConfigSettled {
                    break
                }
            }
            group.cancelAll()
        }
    }

    func waitForChains(_ chainRegistry: ChainRegistryProtocol) async throws {
        try await withThrowingTaskGroup(of: Void.self) { group in
            group.addTask {
                await chainRegistry.asyncWaitChainsSetup(for: Self.requiredChainIds)
            }

            group.addTask {
                try await self.enforceSetupDeadline()
            }

            _ = try await group.next()
            group.cancelAll()
        }
    }

    /// Ends the config wait only when the path is unsatisfied at the offline deadline; online, the config's own retry
    /// budget bounds it.
    func enforceOfflineDeadline() async throws {
        try await clock.sleep(for: .seconds(Constants.offlineSetupDeadlineSeconds))
        guard observer.isPathSatisfied else {
            throw SetupDeadlineExpired()
        }
    }
}
