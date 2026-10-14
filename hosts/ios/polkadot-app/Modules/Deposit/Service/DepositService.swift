import Foundation
import Foundation_iOS
import SubstrateSdk
import AssetExchange
import AsyncExtensions
import Operation_iOS
import CommonService
import StructuredConcurrency
import Coinage
import AssetsManagement
import ChainRegistry

protocol DepositServiceProtocol: AsyncApplicationServicing {
    func fetchDepositInfo(for assetIn: ChainAssetId) async throws -> DepositServiceInfo
    func executions() async -> AnyAsyncSequence<[DepositExecutionItem]>
}

enum DepositServiceError: Error {
    case serviceNotRunning
    case notEnoughFunds
}

actor DepositService {
    struct EstimationSummary {
        let fee: AssetExchangeFee
        let quote: AssetExchangeQuote
    }

    struct ExecutionDetails {
        let summary: EstimationSummary
        let execLabel: DepositExecLabel
    }

    let chainRegistry: ChainRegistryProtocol
    let balanceTrackingFactory: BalanceTrackingFactoryProtocol
    let assetPriceConverter: AssetPriceConverting
    let priceStore: AssetExchangePriceStoring
    let depositCalculator: DepositCalculating
    let coinageService: any CoinageServicing

    nonisolated let logger: LoggerProtocol

    let operationQueue: OperationQueue

    private var allExecutions: AsyncCurrentValueSubject<[DepositExecutionItem]> = .init([])
    private var failedDeposits: Set<DepositExecLabel> = []

    private var isReady = AsyncValue<Void>()

    private let setupModel: AssetExchangeServiceFactoryResult

    private var assetExchangeService: AssetsExchangeServiceProtocol {
        setupModel.service
    }

    private var statsTask: Task<Void, Never>?
    private var executionTask: Task<Void, Never>?
    private var balanceTrackingTask: Task<Void, Never>?
    private var voucherTrackingTask: Task<Void, Never>?

    init(
        assetExchangeFactory: AssetExchangeServiceFactoryProtocol,
        chainRegistry: ChainRegistryProtocol,
        priceStore: AssetExchangePriceStoring,
        balanceTrackingFactory: BalanceTrackingFactoryProtocol,
        coinageService: any CoinageServicing,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) throws {
        let priceConverter = AssetPriceConverter(
            chainRegistry: chainRegistry,
            priceStore: priceStore
        )

        try self.init(
            assetExchangeFactory: assetExchangeFactory,
            chainRegistry: chainRegistry,
            assetPriceConverter: priceConverter,
            priceStore: priceStore,
            depositCalculator: DepositCalculator(
                chainRegistry: chainRegistry,
                assetPriceConverter: priceConverter
            ),
            balanceTrackingFactory: balanceTrackingFactory,
            coinageService: coinageService,
            operationQueue: operationQueue,
            logger: logger
        )
    }

    init(
        assetExchangeFactory: AssetExchangeServiceFactoryProtocol,
        chainRegistry: ChainRegistryProtocol,
        assetPriceConverter: AssetPriceConverting,
        priceStore: AssetExchangePriceStoring,
        depositCalculator: DepositCalculating,
        balanceTrackingFactory: BalanceTrackingFactoryProtocol,
        coinageService: any CoinageServicing,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) throws {
        setupModel = try assetExchangeFactory.createService()
        self.chainRegistry = chainRegistry
        self.assetPriceConverter = assetPriceConverter
        self.priceStore = priceStore
        self.depositCalculator = depositCalculator
        self.balanceTrackingFactory = balanceTrackingFactory
        self.coinageService = coinageService
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension DepositService: DepositServiceProtocol {
    func setup() async {
        await performSetup()
    }

    func throttle() async {
        await performThrottle()
    }

    func fetchDepositInfo(for assetIn: ChainAssetId) async throws -> DepositServiceInfo {
        try await isReady.get()

        let midAmountInAssetIn = try depositCalculator.calculateMid(chainAssetId: assetIn)

        let estimationSummary = try await fetchEstimationSummary(
            assetIn: assetIn,
            assetOut: setupModel.fundedAssetId,
            amountIn: midAmountInAssetIn
        )

        return try calculateDepositInfoWithEstSummary(
            from: estimationSummary,
            assetIn: assetIn,
            walletToDeposit: setupModel.walletToDeposit
        )
    }

    func executions() async -> AnyAsyncSequence<[DepositExecutionItem]> {
        allExecutions.eraseToAnyAsyncSequence()
    }
}

// MARK: - Private Methods

private extension DepositService {
    func performSetup() async {
        setupModel.service.setup()

        startMonitoringStats()
        subscribeVoucherTracking()
    }

    func performThrottle() async {
        throttleDepositChanges()
        throttleVoucherTracking()

        stopMonitoringStats()

        setupModel.service.throttle()

        logger.debug("Deposit service stopped")
    }

    func fetchEstimationSummary(
        assetIn: ChainAssetId,
        assetOut: ChainAssetId,
        amountIn: Balance
    ) async throws -> EstimationSummary {
        let quote = try await assetExchangeService.fetchQuoteWrapper(
            for: AssetConversion.QuoteArgs(
                assetIn: assetIn,
                assetOut: assetOut,
                amount: amountIn,
                direction: .sell
            )
        )
        .asyncExecute()

        let feeArgs = AssetExchangeFeeArgs(
            route: quote.route,
            slippage: DepositServiceConstants.slippage,
            feeAssetId: assetIn,
            destinationAccountId: setupModel.accountToFund
        )

        let fee = try await assetExchangeService.estimateFee(
            for: feeArgs
        )
        .asyncExecute()

        return EstimationSummary(fee: fee, quote: quote)
    }

    func calculateDepositInfoWithEstSummary(
        from estimationSummary: EstimationSummary,
        assetIn: ChainAssetId,
        walletToDeposit: MetaAccountModelProtocol
    ) throws -> DepositServiceInfo {
        let fee = estimationSummary.fee

        let totalFee = fee.calculateTotalFeeInFiat(
            matching: estimationSummary.quote.metaOperations,
            priceStore: priceStore
        )

        let minDeposit = try depositCalculator.calculateMin(
            for: fee,
            chainAssetId: assetIn
        )

        return DepositServiceInfo(
            amountIn: fee.route.amountIn,
            amountOut: fee.route.amountOut,
            feeInUsd: totalFee,
            minDeposit: minDeposit,
            walletToDeposit: walletToDeposit
        )
    }

    func startMonitoringStats() {
        guard statsTask == nil else {
            logger.warning("Already monitoring")
            return
        }

        statsTask = Task { [weak self] in
            guard let service = self?.setupModel.service else {
                return
            }

            let statsStream = service.subscribeUpdates()

            for await stats in statsStream {
                await self?.checkReadiness(for: stats)
            }
        }
    }

    func stopMonitoringStats() {
        statsTask?.cancel()
        statsTask = nil
    }

    func checkReadiness(for stats: AssetsExchangeGraphProviderStats) async {
        let newReadyValue = stats.numberOfLoadedProviders == stats.totalNumberOfProviders
        let oldReadyValue = await isReady.isDefined

        if newReadyValue {
            await isReady.set(())
        } else {
            await isReady.reset()
        }

        if !oldReadyValue, newReadyValue {
            logger.debug("Deposit service is ready")
            subscribeDepositChangesIfNeeded()
        }

        if oldReadyValue, !newReadyValue {
            logger.debug("Providers went offline")
            throttleDepositChanges()
        }
    }

    func subscribeDepositChangesIfNeeded() {
        let isInProgress = allExecutions.value.last?.isInProgress ?? false

        guard !isInProgress else {
            return
        }

        guard balanceTrackingTask == nil else {
            return
        }

        subscribeDepositChanges()
    }

    func subscribeDepositChanges() {
        balanceTrackingTask = Task { [weak self] in
            guard let self else {
                return
            }

            let stream = await balanceTrackingFactory.trackAll(
                for: setupModel.walletToDeposit
            )

            do {
                for try await balance in stream {
                    if let execDetails = await checkBalanceForExecution(balance) {
                        await executeExchange(for: execDetails)
                        break
                    }
                }
            } catch {
                logger.debug("Failed to track balance: \(error)")
            }
        }

        logger.debug("Subscribed to balance changes")
    }

    func throttleDepositChanges() {
        balanceTrackingTask?.cancel()
        balanceTrackingTask = nil
    }

    func checkBalanceForExecution(_ balance: AssetBalance) async -> ExecutionDetails? {
        guard balance.transferable > 0 else {
            return nil
        }

        let execLabel = DepositExecLabel(assetBalance: balance)

        guard !failedDeposits.contains(execLabel) else {
            logger.debug("Balance was already attempted with failure: \(execLabel)")
            return nil
        }

        logger.info(
            "[GameDebug] DepositService detected deposit-wallet balance to onboard: " +
                "asset=\(balance.chainAssetId) transferable=\(balance.transferable)"
        )

        do {
            let preliminarySummary = try await fetchEstimationSummary(
                assetIn: balance.chainAssetId,
                assetOut: setupModel.fundedAssetId,
                amountIn: balance.transferable
            )

            let preliminaryInfo = try calculateDepositInfoWithEstSummary(
                from: preliminarySummary,
                assetIn: balance.chainAssetId,
                walletToDeposit: setupModel.walletToDeposit
            )

            let depositThreshold = DepositServiceConstants.depositInitThreshold.mul(
                value: preliminaryInfo.minDeposit
            )

            guard balance.transferable >= depositThreshold else {
                logger.warning(
                    "[GameDebug] DepositService ignoring balance — below onboard minimum: " +
                        "minimum=\(preliminaryInfo.minDeposit) received=\(balance.transferable)"
                )
                failedDeposits.insert(execLabel)
                return nil
            }

            let chain = try chainRegistry.getChainOrError(for: balance.chainAssetId.chainId)
            let assetIn = try chain.chainAssetInterfaceOrError(for: balance.chainAssetId.assetId)
            let toLeaveAside = preliminarySummary.fee.totalFeeInAssetIn(assetIn)

            let actualDepositAmount = balance.transferable.subtractOrZero(toLeaveAside)

            guard actualDepositAmount > 0 else {
                logger.warning("Ignoring. Not enough for fee: \(balance.transferable) \(toLeaveAside).")
                failedDeposits.insert(execLabel)
                return nil
            }

            let estimationSummary = try await fetchEstimationSummary(
                assetIn: balance.chainAssetId,
                assetOut: setupModel.fundedAssetId,
                amountIn: actualDepositAmount
            )

            return ExecutionDetails(summary: estimationSummary, execLabel: execLabel)
        } catch {
            logger.warning("Execution details fetch failed: \(error)")
            return nil
        }
    }

    func executeExchange(for details: ExecutionDetails) {
        guard prepareForExecution(
            for: details.execLabel,
            fee: details.summary.fee,
            quote: details.summary.quote
        ) else {
            logger.warning("Already executing")
            return
        }

        logger.info("Ready to start exchange")

        executionTask?.cancel()

        executionTask = Task { [weak self] in
            guard let setupModel = self?.setupModel else {
                return
            }

            let submitionStream = setupModel.service.submit(
                using: details.summary.fee,
                creditingTo: setupModel.accountToFund
            )

            for await event in submitionStream {
                guard let self else {
                    break
                }

                switch event {
                case let .inProgress(operationIndex):
                    logger.info("Started execution: \(operationIndex)")

                    await updateExecution(
                        for: details.execLabel,
                        executionIndex: operationIndex,
                        quote: details.summary.quote
                    )
                case let .completed(amount):
                    await completeExecution(for: details.execLabel, result: .success(amount))
                    await subscribeDepositChanges()
                case let .failure(error):
                    await completeExecution(for: details.execLabel, result: .failure(error))
                    await subscribeDepositChanges()
                }
            }
        }
    }

    func prepareForExecution(
        for execLabel: DepositExecLabel,
        fee: AssetExchangeFee,
        quote: AssetExchangeQuote
    ) -> Bool {
        var executions = allExecutions.value
        let notExecuting = allExecutions.value.last?.isFinished ?? true

        guard notExecuting else {
            return false
        }

        let executionItem = DepositExecutionItem(
            execLabel: execLabel,
            amountIn: fee.route.amountIn,
            amountOut: fee.route.amountOut,
            status: .pendingSwap(expectedExecutionTime: quote.totalExecutionTime())
        )

        executions.append(executionItem)

        allExecutions.send(executions)

        logger.debug("Execution prepared for: \(execLabel.chainAssetId)")

        return true
    }

    func updateExecution(
        for execLabel: DepositExecLabel,
        executionIndex: Int,
        quote: AssetExchangeQuote
    ) {
        var executions = allExecutions.value

        guard
            let lastItem = executions.last,
            lastItem.isInProgress else {
            logger.warning("Can't update as no in progress execution")
            return
        }

        guard lastItem.execLabel == execLabel else {
            assertionFailure("Only one execution allowed at a time")
            return
        }

        let remainedTime = quote.totalExecutionTime(from: executionIndex)

        let lastIndex = executions.count - 1
        executions[lastIndex] = lastItem.replacingStatus(
            .inProgress(remainedTime: remainedTime)
        )

        allExecutions.send(executions)
    }

    func completeExecution(for execLabel: DepositExecLabel, result: Result<Balance, Error>) {
        var executions = allExecutions.value

        guard let lastItem = executions.last, lastItem.isInProgress else {
            logger.warning("Can't complete as no in progress execution")
            return
        }

        guard lastItem.execLabel == execLabel else {
            assertionFailure("Can't complete different execution")
            return
        }

        let lastIndex = executions.count - 1

        switch result {
        case let .success(receivedAmount):
            executions[lastIndex] = lastItem.replacingStatus(
                .completed(receivedAmount: receivedAmount)
            )

            logger.info("Exchange completed: \(receivedAmount)")
        case let .failure(error):
            executions[lastIndex] = lastItem.replacingStatus(.failed)

            failedDeposits.insert(execLabel)

            logger.error("Exchange failed: \(error)")
        }

        allExecutions.send(executions)
    }

    func subscribeVoucherTracking() {
        guard voucherTrackingTask == nil else { return }

        voucherTrackingTask = Task { [weak self] in
            guard let self else { return }

            do {
                let chain = try chainRegistry.getChainOrError(for: setupModel.fundedAssetId.chainId)
                let chainAsset = try chain.chainAssetOrError(for: setupModel.fundedAssetId.assetId)
                let stream = await balanceTrackingFactory.trackAccountAsset(
                    setupModel.accountToFund,
                    chainAsset: chainAsset
                )
                for try await balance in stream {
                    await triggerVoucherLoad(balance, chainAsset: chainAsset)
                }
            } catch {
                logger.error("Voucher tracking failed: \(error)")
            }
        }
    }

    func throttleVoucherTracking() {
        voucherTrackingTask?.cancel()
        voucherTrackingTask = nil
    }

    func triggerVoucherLoad(_ balance: AssetBalance, chainAsset _: ChainAsset) async {
        let total = balance.freeInPlank
        guard total > 0 else { return }
        do {
            try await coinageService.loadVouchers(
                amount: total,
                externalAssetHolder: setupModel.walletToDeposit
            )
        } catch {
            logger.error("Failed to load vouchers: \(error)")
        }
    }
}
