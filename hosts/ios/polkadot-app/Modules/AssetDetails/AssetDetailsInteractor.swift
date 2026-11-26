import UIKit
import Operation_iOS
import OperationExt
import Foundation
import SubstrateSdk
import StructuredConcurrency
import Coinage
import CommonService
import AsyncExtensions
import AsyncAlgorithms
import ChainRegistry
import TrUAPIHost
import UIKitExt

final class AssetDetailsInteractor: AnyProviderAutoCleaning {
    weak var presenter: AssetDetailsInteractorOutputProtocol?

    let priceLocalSubscriptionFactory: PriceProviderFactoryProtocol
    let chainAsset: ChainAsset

    private var balanceSubscriptionTask: Task<Void, Error>?
    private var priceProvider: StreamableProvider<PriceData>?
    private var priceSubscriptionTask: Task<Void, Never>?
    private let coinageService: CoinageServicing
    private let coinageBackupSyncService: any CoinageBackupSyncServicing

    private var recoveryStateTask: Task<Void, Error>?
    private var recoveredBalanceTask: Task<Void, Error>?

    private let fundingOpener: FundingOpening
    private var topUpTask: Task<Void, Never>?

    init(
        priceLocalSubscriptionFactory: PriceProviderFactoryProtocol,
        chainAsset: ChainAsset,
        coinageService: CoinageServicing,
        coinageBackupSyncService: any CoinageBackupSyncServicing,
        fundingOpener: FundingOpening
    ) {
        self.priceLocalSubscriptionFactory = priceLocalSubscriptionFactory
        self.chainAsset = chainAsset
        self.coinageService = coinageService
        self.coinageBackupSyncService = coinageBackupSyncService
        self.fundingOpener = fundingOpener
    }

    deinit {
        balanceSubscriptionTask?.cancel()
        recoveryStateTask?.cancel()
        recoveredBalanceTask?.cancel()
        priceSubscriptionTask?.cancel()
        topUpTask?.cancel()
    }
}

extension AssetDetailsInteractor: AssetDetailsInteractorInputProtocol {
    func setup() {
        subscribeToPrice()
        subscribeToBalances()
        subscribeToRecoveryState()
        subscribeToRecoveredBalance()
    }

    func openTopUp() {
        topUpTask?.cancel()
        topUpTask = Task { [weak presenter, fundingOpener] in
            do {
                let session = try await fundingOpener.openFunding(direction: .in)
                await presenter?.didOpenTopUp(.success(session))
            } catch {
                await presenter?.didOpenTopUp(.failure(error))
            }
        }
    }

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
                        holdings: summary.holdings,
                        denominations: context
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
