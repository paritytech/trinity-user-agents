import Foundation
import BigInt
import FoundationExt
import KeyDerivation
import SubstrateSdk
import Operation_iOS
import AsyncExtensions
import SDKLogger
import StateMachine
import StructuredConcurrency

/// Protocol defining the coinage facade operations.
public protocol CoinageServicing: Actor {
    /// The underlying recipient service for direct use.
    nonisolated var ongoingTransferService: any OngoingTransferServicing { get }

    nonisolated var txService: any CoinageTxServicing { get }

    /// Claims coins a peer handed us, driven off the durability group `groupId = messageId`.
    nonisolated var claimCoinsService: any ClaimCoinsServicing { get }

    /// The Appendix-A derived payment status of coins we handed off.
    nonisolated var transferStatusService: any CoinageTransferStatusServicing { get }

    /// The external payment service — exposed for dependency registration.
    /// Lifecycle (setup/throttle) is managed internally by CoinageService.
    nonisolated var externalPaymentService: any ExternalPaymentServicing { get }

    /// The incoming-payment (top-up) service — exposed for dependency registration.
    /// Lifecycle (setup/throttle) is managed internally by CoinageService.
    nonisolated var incomingPaymentService: any IncomingPaymentServicing { get }

    /// Suspends until the denomination context is ready, then returns it.
    /// Throws if `setup(with:)` has not been called or if setup failed.
    func denominationContext() async throws -> DenominationBreakdownContext

    /// Configure the facade with an asset. Must be called before other operations.
    /// - Parameter asset: The asset providing decimal precision
    /// - Throws: Errors from context loading
    func setup(with asset: AssetProtocol) async throws

    /// Load vouchers for a given amount, signed by the wallet that holds the external asset.
    /// Suspends and waits if the service has not been configured with an asset yet.
    /// - Parameters:
    ///   - amount: The fiat amount to load into vouchers
    ///   - externalAssetHolder: Wallet whose origin signs the on-chain extrinsic and whose
    ///     external asset is being onboarded into vouchers
    /// - Returns: The total value actually loaded, in planks.
    /// - Throws: CoinageError on failure
    @discardableResult
    func loadVouchers(
        amount: BigUInt,
        externalAssetHolder: any WalletManaging
    ) async throws -> BigUInt

    /// Provides a service to stream total and locked balance updates.
    /// Suspends and waits if the service has not been configured with an asset yet.
    func coinageBalanceService() async throws -> CoinageBalanceServiceProtocol

    /// Preview an external payment: what it would spend and whether that costs privacy.
    func previewExternalPayment(for amount: BigUInt) async throws -> ExternalPaymentPreview

    /// Whether private vouchers alone would pay `amount` — the check behind the privacy warning.
    func canExecuteExternalPaymentPrivately(amount: BigUInt) async throws -> Bool

    /// Register an external payment identified by `(productId, paymentId)`.
    /// Throws `ExternalPaymentError.alreadyExists` on replay.
    func initiateExternalPayment(
        productId: String,
        paymentId: String,
        amountInPlanks: Balance,
        destination: AccountId
    ) async throws

    /// Subscribe to the status of an external payment identified by `(productId, paymentId)`; the
    /// stream fails with `ExternalPaymentError.notFound` for an unknown identity.
    nonisolated func subscribeExternalPaymentStatus(
        productId: String,
        paymentId: String
    ) -> AnyAsyncSequence<ExternalPaymentStatus>

    /// Preview a transfer and compute both the full and non-degraded sendable amounts.
    func previewTransfer(for amount: BigUInt) async throws -> TransferPreview

    /// Execute a transfer from a pre-computed coin selection result, skipping coin selection.
    /// Returns the memo plus the provisional handoff to commit once the memo is durable.
    /// `groupId` labels the registered transaction(s) — the transfer's message id, or `nil`.
    func executeTransfer(result: CoinSelectionResult, groupId: CoinageTxGroupId) async throws -> PreparedTransfer

    /// Where recovery of previous installations' balance stands — see ``BackupProgress``.
    nonisolated func subscribeBackupProgress() -> AnyAsyncSequence<BackupProgress>

    /// Another look for balance under previous installations, past where the launch scan stopped.
    func deepSearchBackup() async

    /// The user accepted the recovered balance; the progress becomes ``BackupProgress/completed``.
    func markBackupAsCompleted() async

    /// Whether this installation is registered on chain — see ``CoinageAccountBackupStatus``.
    nonisolated func subscribeAccountBackupStatus() -> AnyAsyncSequence<CoinageAccountBackupStatus>

    /// See `TransferClaimServicing.transferCoinsFromSecretKeys`.
    func transferCoinsFromSecretKeys(
        secretKeys: [Data],
        transferCoins: Bool
    ) async throws -> BigUInt

    /// Recover spent coins by re-sweeping them back into the user's balance.
    /// Enumerates locally spent coins and delegates to
    /// `OngoingTransferServicing.recoverSpentCoins(spentCoins:context:)`, which transfers
    /// younger on-chain coins and restores max-age ones to `.available` locally.
    /// - Returns: Total planks recovered (transferred + restored).
    /// - Throws: Errors from coin enumeration, key derivation, or transfer execution.
    func recoverSpentCoinsOnChain() async throws -> BigUInt
}

/// Facade that coordinates transfer execution and balance queries across services.
public actor CoinageService {
    // Coins and vouchers management
    private let coinService: CoinServiceProtocol
    private let voucherService: VoucherServiceProtocol
    private let coinKeypairFactory: any CoinKeyDeriving

    // Transfers
    private let senderService: TransferSenderServicing
    public nonisolated let ongoingTransferService: any OngoingTransferServicing
    public nonisolated let txService: any CoinageTxServicing
    public nonisolated let claimCoinsService: any ClaimCoinsServicing
    public nonisolated let transferStatusService: any CoinageTransferStatusServicing

    // Sync services
    private let coinStateSyncService: CoinStateSyncService
    private let voucherLocationService: VoucherLocationService
    private let recoveryService: any CoinageBackupRecoveryServicing
    private let installationRegistrar: any CoinageInstallationRegistering
    public nonisolated let recyclingService: any CoinageRecyclingServicing

    // Recycling strategy evaluation — the evaluator is built lazily once the context resolves.
    private nonisolated let recyclingStrategySettings: any CoinageRecyclingStrategyProviding
    private let recyclingStrategyResolver: any RecyclingStrategyProviding
    private let ringCapacityProvider: any RingCapacityProviding
    private let preClassificator: any CoinageAssetsPreClassificating

    // External payment — lifecycle managed internally, exposed for dependency registration
    public nonisolated let externalPaymentService: any ExternalPaymentServicing

    // Incoming payments (top-ups) — lifecycle managed internally (setup driven by `setup(with:)`),
    // exposed for dependency registration. Mirrors `externalPaymentService`.
    public nonisolated let incomingPaymentService: any IncomingPaymentServicing

    private let contextLoader: DenominationContextLoaderProtocol

    // Balance observation — the factory builds the tracked-asset snapshot streams on demand
    private let databaseFactory: any DatabaseDependencyFactoring
    private let logger: SDKLoggerProtocol?

    // App State
    private let applicationStateStreamFactory: ApplicationStateStreamFactory
    private var recyclingEvaluator: CoinRecyclingEvaluator?

    private var breakdownContext: DenominationBreakdownContext?
    private var cachedBalanceService: CoinageBalanceServiceProtocol?

    /// Cached task for the first context fetch. Concurrent callers await the same task;
    /// cancelled tasks propagate naturally without leaking continuations.
    /// Reset to nil on failure to allow retry via a subsequent `setup(with:)` call.
    private var contextSetupTask: Task<DenominationBreakdownContext, Error>?
    /// The asset associated with `contextSetupTask`. Used to detect asset changes
    /// while a fetch is in-flight so the task is replaced rather than reused.
    private var contextSetupAssetId: AssetId?
    /// Broadcasts context results to callers that arrived before setup() created contextSetupTask.
    /// nil = not yet set up; .success = ready; .failure = last setup failed (reset to nil on retry).
    private let contextSubject = AsyncCurrentValueSubject<Result<DenominationBreakdownContext, Error>?>(nil)

    init(
        coinService: CoinServiceProtocol,
        voucherService: VoucherServiceProtocol,
        coinKeypairFactory: any CoinKeyDeriving,
        senderService: TransferSenderServicing,
        ongoingTransferService: any OngoingTransferServicing,
        txService: any CoinageTxServicing,
        claimCoinsService: any ClaimCoinsServicing,
        transferStatusService: any CoinageTransferStatusServicing,
        externalPaymentService: any ExternalPaymentServicing,
        contextLoader: DenominationContextLoaderProtocol,
        coinStateSyncService: CoinStateSyncService,
        voucherLocationService: VoucherLocationService,
        recyclingService: any CoinageRecyclingServicing,
        recyclingStrategySettings: any CoinageRecyclingStrategyProviding,
        recyclingStrategyResolver: any RecyclingStrategyProviding,
        ringCapacityProvider: any RingCapacityProviding,
        preClassificator: any CoinageAssetsPreClassificating,
        applicationStateStreamFactory: ApplicationStateStreamFactory,
        databaseFactory: any DatabaseDependencyFactoring,
        recoveryService: any CoinageBackupRecoveryServicing,
        installationRegistrar: any CoinageInstallationRegistering,
        incomingPaymentService: any IncomingPaymentServicing,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.coinService = coinService
        self.voucherService = voucherService
        self.coinKeypairFactory = coinKeypairFactory
        self.senderService = senderService
        self.ongoingTransferService = ongoingTransferService
        self.externalPaymentService = externalPaymentService
        self.contextLoader = contextLoader
        self.coinStateSyncService = coinStateSyncService
        self.voucherLocationService = voucherLocationService
        self.recyclingService = recyclingService
        self.recyclingStrategySettings = recyclingStrategySettings
        self.recyclingStrategyResolver = recyclingStrategyResolver
        self.ringCapacityProvider = ringCapacityProvider
        self.preClassificator = preClassificator
        self.applicationStateStreamFactory = applicationStateStreamFactory
        self.databaseFactory = databaseFactory
        self.recoveryService = recoveryService
        self.installationRegistrar = installationRegistrar
        self.txService = txService
        self.claimCoinsService = claimCoinsService
        self.transferStatusService = transferStatusService
        self.incomingPaymentService = incomingPaymentService
        self.logger = logger
    }
}

// MARK: - CoinageServicing

extension CoinageService: CoinageServicing {
    // MARK: External Payment Delegation

    public func previewExternalPayment(for amount: BigUInt) async throws -> ExternalPaymentPreview {
        let context = try await requireContext()
        return try await externalPaymentService.previewPayment(for: amount, context: context)
    }

    public func canExecuteExternalPaymentPrivately(amount: BigUInt) async throws -> Bool {
        let context = try await requireContext()
        return try await externalPaymentService.canExecuteExternalPaymentPrivately(amount: amount, context: context)
    }

    public func initiateExternalPayment(
        productId: String,
        paymentId: String,
        amountInPlanks: Balance,
        destination: AccountId
    ) async throws {
        try await externalPaymentService.initiatePayment(
            productId: productId,
            paymentId: paymentId,
            amountInPlanks: amountInPlanks,
            destination: destination
        )
    }

    public nonisolated func subscribeExternalPaymentStatus(
        productId: String,
        paymentId: String
    ) -> AnyAsyncSequence<ExternalPaymentStatus> {
        externalPaymentService.subscribePaymentStatus(productId: productId, paymentId: paymentId)
    }

    // MARK: Denomination Context

    public func denominationContext() async throws -> DenominationBreakdownContext {
        try await requireContext()
    }

    public func setup(with asset: AssetProtocol) async throws {
        do {
            let context: DenominationBreakdownContext
            if let existing = breakdownContext {
                // Re-setup: update precision synchronously, no fetch needed
                context = existing.withChanging(asset: asset)
            } else {
                // First setup: create a shared task so concurrent callers
                // await the same fetch rather than each registering a continuation.
                let task: Task<DenominationBreakdownContext, Error>
                if let existing = contextSetupTask, contextSetupAssetId == asset.assetId {
                    task = existing
                } else {
                    // Reset subject so subject-path waiters from a prior failed attempt
                    // resume waiting rather than seeing a stale .failure result.
                    contextSubject.send(nil)
                    task = Task { [contextLoader] in try await contextLoader.fetchContext(for: asset) }
                    contextSetupTask = task
                    contextSetupAssetId = asset.assetId
                }
                let fetched = try await task.value
                // Guard against reentrancy: another setup call may have set breakdownContext
                // while we were suspended awaiting the task.
                guard breakdownContext == nil else { return }
                context = fetched
            }
            breakdownContext = context
            contextSubject.send(.success(context))

            // Start sync services
            coinStateSyncService.setup()
            voucherLocationService.setup()
            externalPaymentService.setup(with: context)
            incomingPaymentService.setup(with: context)

            ensureRecyclingEvaluator(context: context)

            // Before the engine's first recovery pass, which the app starts once setup returns, so nothing
            // is decided against a mark that is about to disappear.
            try await txService.releaseUncommittedHandoffs()

            await startInstallationBackup()

        } catch {
            // Reset so a subsequent setup(with:) call triggers a fresh fetch
            contextSetupTask = nil
            contextSetupAssetId = nil
            contextSubject.send(.failure(error))
            throw error
        }
    }

    // MARK: Coinage Transfers

    public func transferCoinsFromSecretKeys(
        secretKeys: [Data],
        transferCoins: Bool
    ) async throws -> BigUInt {
        let context = try await requireContext()
        return try await ongoingTransferService.transferCoinsFromSecretKeys(
            secretKeys: secretKeys,
            transferCoins: transferCoins,
            context: context
        )
    }

    public func previewTransfer(for amount: BigUInt) async throws -> TransferPreview {
        guard let denominationContext = breakdownContext else {
            throw CoinageError.notConfigured
        }

        let coins = try await coinService.fetchAllTrackedCoins()
        let vouchers = try await voucherService.fetchAllTracked()

        // Spendable first; widen to gaining-privacy funds only if spendable cannot cover the amount.
        // Under `maxPrivacy` the selector never widens, so the second pass is a no-op and the loop
        // still terminates in `insufficientFunds`.
        for scope in [SpendScope.spendable, .withConfirmation] {
            let (availableCoins, availableVouchers) = await selectableAssets(
                coins: coins,
                vouchers: vouchers,
                scope: scope
            )

            do {
                let result = try await senderService.previewStrategy(
                    amount: amount,
                    availableCoins: availableCoins,
                    availableVouchers: availableVouchers,
                    breakdownContext: denominationContext
                )
                return TransferPreview(selectionResult: result, fullAmount: amount, scope: scope)
            } catch CoinSelectionError.insufficientFunds, CoinSelectionError.emptyWallet {
                continue
            }
        }

        throw CoinSelectionError.insufficientFunds
    }

    public func executeTransfer(
        result: CoinSelectionResult,
        groupId: CoinageTxGroupId
    ) async throws -> PreparedTransfer {
        guard let denominationContext = breakdownContext else {
            throw CoinageError.notConfigured
        }

        do {
            return try await senderService.execute(
                result: result,
                breakdownContext: denominationContext,
                groupId: groupId
            )
        } catch {
            throw CoinageError.transferFailed(underlying: error)
        }
    }

    // MARK: Vouchers

    @discardableResult
    public func loadVouchers(
        amount: BigUInt,
        externalAssetHolder: any WalletManaging
    ) async throws -> BigUInt {
        try await markStallRegion("Loading vouchers") {
            let context = try await requireContext()
            let vouchers = try await voucherService.load(
                amount: amount,
                externalAssetHolder: externalAssetHolder,
                breakdownContext: context,
                groupId: nil
            )
            return vouchers.reduce(BigUInt.zero) { $0 + context.valueInPlanks(for: $1.exponent) }
        }
    }

    public func coinageBalanceService() async throws -> CoinageBalanceServiceProtocol {
        if let service = cachedBalanceService {
            return service
        }
        let context = try await requireContext()
        // Re-check after suspension — setup may have created the service concurrently
        if let service = cachedBalanceService {
            return service
        }
        let evaluator = ensureRecyclingEvaluator(context: context)
        let service = CoinageBalanceService(
            denominationContext: context,
            databaseFactory: databaseFactory,
            verdicts: evaluator.verdicts,
            settings: recyclingStrategySettings,
            strategyResolver: recyclingStrategyResolver,
            ringCapacityProvider: ringCapacityProvider,
            preClassificator: preClassificator,
            logger: logger
        )
        service.start()
        cachedBalanceService = service
        return service
    }

    // MARK: Recovery

    public nonisolated func subscribeBackupProgress() -> AnyAsyncSequence<BackupProgress> {
        recoveryService.subscribeProgress()
    }

    public func deepSearchBackup() async {
        await recoveryService.deepSearch()
    }

    public func markBackupAsCompleted() async {
        await recoveryService.markAsCompleted()
    }

    public nonisolated func subscribeAccountBackupStatus() -> AnyAsyncSequence<CoinageAccountBackupStatus> {
        installationRegistrar.subscribeStatus()
    }

    public func recoverSpentCoinsOnChain() async throws -> BigUInt {
        let spentCoins = try await coinService.fetchAllTrackedCoins()
            .filter(\.isRecoverable)
            .map(\.coin)
        guard !spentCoins.isEmpty else { return .zero }
        let context = try await denominationContext()
        return try await ongoingTransferService.recoverSpentCoins(
            spentCoins: spentCoins,
            context: context
        )
    }
}

// MARK: - Installation backup

private extension CoinageService {
    /// Registers this installation and recovers the previous ones. `setup(with:)` may run again to update
    /// the asset precision; both services run once per process and ignore later starts.
    func startInstallationBackup() async {
        installationRegistrar.start()
        await recoveryService.start()
    }
}

// MARK: - Context Access

private extension CoinageService {
    /// Returns context immediately if available, or suspends until setup() posts a result.
    /// Propagates setup errors. Throws CancellationError if the task is cancelled while waiting.
    func requireContext() async throws -> DenominationBreakdownContext {
        if let existing = breakdownContext {
            return existing
        }
        for await result in contextSubject {
            guard let result else { continue }
            return try result.get()
        }
        throw CancellationError()
    }
}

// MARK: - Recycling Evaluation

private extension CoinageService {
    /// Lazily builds and starts the verdict-driven recycling evaluator — the single foreground trigger
    /// for recycling, replacing the age-based schedule and the foreground catch-up. Shared with the
    /// balance service, which consumes its verdicts. Built here because it needs the resolved context.
    @discardableResult
    func ensureRecyclingEvaluator(context: DenominationBreakdownContext) -> CoinRecyclingEvaluator {
        if let recyclingEvaluator {
            return recyclingEvaluator
        }

        let evaluator = CoinRecyclingEvaluator(
            databaseFactory: databaseFactory,
            settings: recyclingStrategySettings,
            strategyProvider: recyclingStrategyResolver,
            ringCapacityProvider: ringCapacityProvider,
            preClassificator: preClassificator,
            recyclingService: recyclingService,
            denominationContext: context,
            logger: logger
        )
        evaluator.start()
        recyclingEvaluator = evaluator
        return evaluator
    }

    /// Narrows the wallet to the assets a spend may draw on for `scope`, applying the current verdicts
    /// and voucher usability. Before the first verdict lands it returns the raw sets, letting the
    /// downstream `CoinSelector` apply its own free/on-chain filter.
    func selectableAssets(
        coins: [TrackedCoin],
        vouchers: [TrackedVoucher],
        scope: SpendScope
    ) async -> (coins: [TrackedCoin], vouchers: [TrackedVoucher]) {
        guard let verdicts = recyclingEvaluator?.currentVerdicts() else {
            return (coins, vouchers)
        }

        let voucherStrategy = recyclingStrategyResolver.voucherStrategy(for: recyclingStrategySettings.strategy)
        let capacities = await (try? ringCapacityProvider.capacities(
            for: Set(vouchers.map(\.voucher.exponent))
        )) ?? [:]
        let usability = VoucherUsabilityContext(ringCapacities: capacities, now: Date())

        let selector = CoinageAssetSelector(preClassificator: preClassificator)
        return (
            coins: selector.selectableCoins(
                coins,
                verdicts: verdicts,
                allowsConfirmedSpend: voucherStrategy.allowsConfirmedSpend(),
                scope: scope
            ),
            vouchers: selector.selectableVouchers(
                vouchers,
                strategy: voucherStrategy,
                context: usability,
                scope: scope
            )
        )
    }
}
