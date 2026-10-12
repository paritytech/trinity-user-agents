import UIKit
import Operation_iOS
import OperationExt
import Foundation
import SubstrateSdk
import StructuredConcurrency
import Coinage
import CommonService
import KeyDerivation
import AsyncExtensions
import AsyncAlgorithms
import ChainRegistry
import BackgroundExecution
import Products
import UIKitExt

final class AssetDetailsInteractor: AnyProviderAutoCleaning {
    weak var presenter: AssetDetailsInteractorOutputProtocol?

    let priceLocalSubscriptionFactory: PriceProviderFactoryProtocol
    let chainAsset: ChainAsset

    private let fiatOnrampTrackingService: FiatOnrampTrackingServiceProtocol
    private var fiatOnrampTrackingTask: Task<Void, Never>?

    private var balanceSubscriptionTask: Task<Void, Error>?
    private var priceProvider: StreamableProvider<PriceData>?
    private var priceSubscriptionTask: Task<Void, Never>?
    private let coinageService: CoinageServicing
    private let coinageBackupSyncService: any CoinageBackupSyncServicing

    private var recoveryStateTask: Task<Void, Error>?
    private var recoveredBalanceTask: Task<Void, Error>?
    private var accountBackupStatusTask: Task<Void, Error>?

    private let fundingDomainProvider: FundingDomainProviding
    private var rampProductTasks: [RampAction: Task<Void, Never>] = [:]

    #if TESTNET_FEATURE
        var backgroundExecutor: BackgroundExecuting?
        var topupService: TopUpService?
        var faucetTask: Task<Void, Error>?
    #endif

    init(
        priceLocalSubscriptionFactory: PriceProviderFactoryProtocol,
        fiatOnrampTrackingService: FiatOnrampTrackingServiceProtocol,
        chainAsset: ChainAsset,
        coinageService: CoinageServicing,
        coinageBackupSyncService: any CoinageBackupSyncServicing,
        fundingDomainProvider: FundingDomainProviding
    ) {
        self.priceLocalSubscriptionFactory = priceLocalSubscriptionFactory
        self.fiatOnrampTrackingService = fiatOnrampTrackingService
        self.chainAsset = chainAsset
        self.coinageService = coinageService
        self.coinageBackupSyncService = coinageBackupSyncService
        self.fundingDomainProvider = fundingDomainProvider
    }

    deinit {
        fiatOnrampTrackingTask?.cancel()
        balanceSubscriptionTask?.cancel()
        recoveryStateTask?.cancel()
        recoveredBalanceTask?.cancel()
        accountBackupStatusTask?.cancel()
        priceSubscriptionTask?.cancel()
        rampProductTasks.values.forEach { $0.cancel() }
    }
}

extension AssetDetailsInteractor: AssetDetailsInteractorInputProtocol {
    func setup() {
        subscribeToFiatOnrampTracking()
        subscribeToPrice()
        subscribeToBalances()
        subscribeToRecoveryState()
        subscribeToRecoveredBalance()
        subscribeToAccountBackupStatus()
    }

    func triggerSync() {
        coinageBackupSyncService.triggerRecovery()
    }

    func cancelBackupNotification() {
        coinageBackupSyncService.acknowledgeRecovery()
    }

    func removeCompletedFiatOnrampTransactions() {
        fiatOnrampTrackingService.removeCompletedTransactions()
    }

    func removeFailedFiatOnrampTransactions() {
        fiatOnrampTrackingService.removeFailedTransactions()
    }

    func openRampProduct(_ action: RampAction) {
        rampProductTasks[action]?.cancel()
        rampProductTasks[action] = Task { [weak presenter, fundingDomainProvider] in
            do {
                let page = try await action.resolvePage(using: fundingDomainProvider)
                await presenter?.didResolveRampProduct(action, result: .success(page))
            } catch {
                await presenter?.didResolveRampProduct(action, result: .failure(error))
            }
        }
    }

    #if TESTNET_FEATURE
        func topUp() {
            faucetTask?.cancel()
            faucetTask = Task { [weak presenter, topupService, backgroundExecutor, coinageService] in
                guard let topupService, let backgroundExecutor else {
                    return
                }
                do {
                    guard let amount = Decimal(5).toSubstrateAmount(precision: chainAsset.asset.decimalPrecision) else {
                        return
                    }

                    let randomSeed = try Data.randomOrError(of: 32)
                    let depositWallet = try DynamicDerivedWallet(seedBytes: randomSeed)

                    try await backgroundExecutor.execute {
                        try await markStallActivity("Topup") {
                            try await topupService.topUp(depositWallet, amount: .plank(amount))
                            try await coinageService.loadVouchers(amount: amount, externalAssetHolder: depositWallet)
                        }
                    }

                    await presenter?.didCompleteTopUp(.success(()))
                } catch {
                    await presenter?.didCompleteTopUp(.failure(error))
                }
            }
        }
    #endif

    /// Reads the balance and the holdings behind it as one value. Two subscriptions would let the
    /// figures and the rows come from different evaluations, so the breakdown would briefly show
    /// totals its own rows do not add up to.
    private func subscribeToBalances() {
        balanceSubscriptionTask?.cancel()
        balanceSubscriptionTask = Task { [weak self] in
            guard let self else { return }
            do {
                let balanceService = try await coinageService.coinageBalanceService()
                let context = balanceService.denominationContext
                for try await summary in balanceService.summaryStream {
                    try Task.checkCancellation()
                    let balance = summary.balance
                    await presenter?.didReceive(balance: context.decimal(fromPlanks: balance.total))

                    // Ready and Clearing come from the one place that collapses the domain's
                    // three buckets into the two the user is shown, so the figures, the bar and
                    // the coins cannot disagree, and the two add up to the total above them.
                    await presenter?.didReceive(
                        coinageAmounts: CoinageAmounts(
                            total: context.decimal(fromPlanks: balance.total),
                            availableNow: context.decimal(fromPlanks: balance.ready),
                            gainingPrivacy: context.decimal(fromPlanks: balance.clearing)
                        ),
                        holdings: summary.holdings
                    )
                }
            } catch {
                Logger.shared.error("Balance stream failed: \(error)")
            }
        }
    }
}

private extension AssetDetailsInteractor {
    func subscribeToRecoveryState() {
        recoveryStateTask?.cancel()
        recoveryStateTask = Task { [weak presenter, coinageBackupSyncService] in
            for try await inProgress in coinageBackupSyncService.isRecoveryInProgressStream {
                await presenter?.didReceive(isRecoveryInProgress: inProgress)
            }
        }
    }

    func subscribeToRecoveredBalance() {
        recoveredBalanceTask?.cancel()
        recoveredBalanceTask = Task { [weak presenter, coinageBackupSyncService] in
            for try await shows in coinageBackupSyncService.showsRecoveredBalanceStream {
                await presenter?.didReceive(showsRecoveredBalance: shows)
            }
        }
    }

    func subscribeToAccountBackupStatus() {
        accountBackupStatusTask?.cancel()
        accountBackupStatusTask = Task { [weak presenter, coinageService] in
            for try await status in coinageService.subscribeAccountBackupStatus() {
                await presenter?.didReceive(isAccountBackupPending: status.needsAttention)
            }
        }
    }

    func subscribeToFiatOnrampTracking() {
        fiatOnrampTrackingTask?.cancel()
        fiatOnrampTrackingTask = Task { [weak self] in
            guard let self else {
                return
            }

            let stream = await fiatOnrampTrackingService.subscribeToTransactionStatuses()

            do {
                for try await statuses in stream {
                    await presenter?.didReceive(fiatOnrampStatuses: statuses)
                }
            } catch {
                // No-op: tracking updates are best-effort
            }
        }
    }

    private func subscribeToPrice() {
        guard let priceId = chainAsset.asset.priceId else {
            return
        }
        priceProvider = priceLocalSubscriptionFactory.getPriceStreamableProvider(
            for: priceId,
            currency: .usd
        )

        priceSubscriptionTask?.cancel()
        priceSubscriptionTask = Task { [weak self] in
            guard let self, let priceProvider else { return }
            do {
                for try await changes in priceProvider.asyncStream() {
                    let price = changes.reduceToLastChange()
                    await MainActor.run {
                        self.presenter?.didReceive(price: price)
                    }
                }
            } catch {
                Logger.shared.error("Price subscription failed: \(error)")
            }
        }
    }
}
