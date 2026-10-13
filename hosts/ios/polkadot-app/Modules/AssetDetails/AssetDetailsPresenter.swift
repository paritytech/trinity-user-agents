import BigInt
import Foundation
import Foundation_iOS
import SubstrateSdk
import PolkadotUI
import Coinage
import ChainRegistry
import Products

@MainActor
final class AssetDetailsPresenter {
    weak var view: AssetDetailsViewProtocol?

    let wireframe: AssetDetailsWireframeProtocol
    let interactor: AssetDetailsInteractorInputProtocol?

    let viewModelFactory: WalletCardViewModelFactoryProtocol
    let paymentAssetViewModelFactory: PaymentAssetViewModelMaking
    private let balanceFormatterFactory: AssetBalanceFormatterFactoryProtocol
    private var balanceFormatter: LocalizableDecimalFormatting?
    private var priceFormatter: LocalizableDecimalFormatting?
    private lazy var fundingConfiguration: AssetFundingStatusView.Configuration =
        .fundingDigitalDollarConfiguration()

    private let chainAsset: ChainAsset
    private var balance: Decimal = 0
    /// Classified alongside the balance figures, so the coins and the figures always account for
    /// exactly the total shown above them.
    private var holdings: CoinageHoldings = .empty
    /// Ready and Clearing for the real holdings, totalled by the balance service.
    private var coinageAmounts: CoinageAmounts?
    /// Loaded from chain state; the balance is shown against it.
    private var price: PriceData?
    let logger: LoggerProtocol

    init(
        interactor: AssetDetailsInteractorInputProtocol,
        wireframe: AssetDetailsWireframeProtocol,
        viewModelFactory: WalletCardViewModelFactoryProtocol,
        logger: LoggerProtocol,
        chainAsset: ChainAsset,
        balanceFormatterFactory: AssetBalanceFormatterFactoryProtocol = AssetBalanceFormatterFactory(),
        paymentAssetViewModelFactory: PaymentAssetViewModelMaking = PaymentAssetViewModelFactory()
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        self.logger = logger
        self.chainAsset = chainAsset
        self.balanceFormatterFactory = balanceFormatterFactory
        self.paymentAssetViewModelFactory = paymentAssetViewModelFactory
    }

    private func provideAssets() {
        let viewModels = [viewModelFactory.createAssetViewModel(from: chainAsset)]
        view?.didSetCards(viewModels: viewModels)
    }

    private func provideAssetBalance() {
        // One configuration for every figure this screen shows: the fiat symbol in front, and the
        // asset symbol left to whoever sets the figure, since only some of them name it.
        let balanceViewModelFactory = PrimitiveBalanceViewModelFactory(
            targetAssetInfo: chainAsset.asset.digitalDollarFigureDisplayInfo,
            formatterFactory: balanceFormatterFactory
        )

        let balanceViewModel = balanceViewModelFactory.balanceFromPrice(
            balance,
            priceData: price
        )
        .value(for: .current)

        view?.didReceiveData(viewModel: .token(balanceViewModel), index: 0)

        guard let coinageAmounts, coinageAmounts.hasFundsNotReady else {
            view?.didReceive(readyAmount: nil)
            return
        }

        let readyViewModel = balanceViewModelFactory.balanceFromPrice(
            coinageAmounts.availableNow,
            priceData: price
        )
        .value(for: .current)

        view?.didReceive(readyAmount: readyViewModel)
    }
}

extension AssetDetailsPresenter: AssetDetailsPresenterProtocol {
    func onBackupSync() {
        interactor?.triggerSync()
    }

    func onBackupCancel() {
        wireframe.showCancelBackupConfirmation(from: view) { [weak self] in
            self?.interactor?.cancelBackupNotification()
        }
    }

    func onBackupWhyUpdate() {
        wireframe.showWhyBackupUpdate(from: view)
    }

    func setup() {
        provideAssets()
        provideAssetBalance()
        view?.didReceive(paymentAsset: paymentAssetViewModelFactory.makeViewModel())
        interactor?.setup()
    }

    func onSendMoney() {
        wireframe.showTransfer(from: view, chainAsset: chainAsset)
    }

    func onAddMoney() {
        wireframe.showAddTokens(from: view)
    }

    func onFundingCompletedAction() {
        interactor?.removeCompletedFiatOnrampTransactions()
    }

    func onFundingFailedAction() {
        interactor?.removeFailedFiatOnrampTransactions()
    }

    func onTopUp() {
        openRampProduct(.topUp)
    }

    func onWithdraw() {
        openRampProduct(.withdraw)
    }

    #if TESTNET_FEATURE
        func onTestnetTopUp() {
            view?.didReceive(testnetTopUpLoading: true)

            interactor?.topUp()
        }
    #endif
}

extension AssetDetailsPresenter: AssetDetailsInteractorOutputProtocol {
    func didResolveRampProduct(_ action: RampAction, result: Result<ProductPage, Error>) {
        view?.didReceive(rampLoading: action, isLoading: false)

        switch result {
        case let .success(page):
            wireframe.showProduct(page: page)
        case let .failure(error):
            guard !(error is CancellationError) else { return }
            wireframe.present(error: action.errorContent(for: error), from: view)
        }
    }

    func didReceive(coinageAmounts: CoinageAmounts, holdings: CoinageHoldings) {
        self.coinageAmounts = coinageAmounts
        self.holdings = holdings
        provideAssetBalance()
        provideCoinageBreakdown()
    }

    #if TESTNET_FEATURE
        func didCompleteTopUp(_ result: Result<Void, Error>) {
            view?.didReceive(testnetTopUpLoading: false)

            guard case let .failure(error) = result else {
                return
            }

            wireframe.present(error: error, from: view)
        }
    #endif

    func didReceive(fiatOnrampStatuses: Set<FiatOnrampTransactionStatusPayload>) {
        provideFundingStates(from: fiatOnrampStatuses)
    }

    func didReceive(balance: Decimal) {
        self.balance = balance
        provideAssetBalance()
        #if TESTNET_FEATURE
            provideCoinageBreakdown()
        #endif
    }

    func didReceive(price: PriceData?) {
        self.price = price
        provideAssetBalance()
    }

    func didReceive(isRecoveryInProgress: Bool) {
        view?.didReceive(isRecoveryInProgress: isRecoveryInProgress)
    }

    func didReceive(isAccountBackupPending: Bool) {
        view?.didReceive(isAccountBackupPending: isAccountBackupPending)
    }

    func didReceive(showsRecoveredBalance: Bool) {
        if showsRecoveredBalance {
            view?.didShowBackupNotification()
        } else {
            view?.didHideBackupNotification()
        }
    }
}

private extension AssetDetailsPresenter {
    func provideFundingStates(from statuses: Set<FiatOnrampTransactionStatusPayload>) {
        let sortedStatuses = statuses.sorted { lhs, rhs in
            lhs.id.value < rhs.id.value
        }

        let states: [AssetFundingStatusView.FundingState] = sortedStatuses.map { status in
            switch status.status {
            case .funding:
                .init(id: status.id.value, status: .waiting)
            case let .inProgress(remainedTime, inAmount, outAmount):
                .init(
                    id: status.id.value,
                    status: .inProgress(
                        totalSeconds: Int(ceil(remainedTime)),
                        amountIn: formatAmount(inAmount),
                        amountOut: formatUsdAmount(outAmount)
                    )
                )
            case let .completed(inAmount, outAmount):
                .init(
                    id: status.id.value,
                    status: .completed(
                        amountIn: formatAmount(inAmount),
                        amountOut: formatUsdAmount(outAmount)
                    )
                )
            case .failed:
                .init(id: status.id.value, status: .failed)
            }
        }

        view?.didReceive(fundingStates: states)
    }

    func formatAmount(_ amount: Balance) -> String {
        let formatter = getBalanceFormatter()
        let decimalAmount = amount.decimal(assetInfo: chainAsset.assetDisplayInfo)
        return formatter.stringFromDecimal(decimalAmount) ?? ""
    }

    func getBalanceFormatter() -> LocalizableDecimalFormatting {
        if let balanceFormatter {
            return balanceFormatter
        }

        let formatter = balanceFormatterFactory
            .createTokenFormatter(for: chainAsset.assetDisplayInfo)
            .value(for: .current)

        balanceFormatter = formatter

        return formatter
    }

    func getPriceFormatter() -> LocalizableDecimalFormatting {
        if let priceFormatter {
            return priceFormatter
        }

        let formatter = balanceFormatterFactory
            .createAssetPriceFormatter(for: .usd)
            .value(for: .current)

        priceFormatter = formatter

        return formatter
    }

    func formatUsdAmount(_ amount: Balance) -> String {
        let formatter = getPriceFormatter()
        let decimalAmount = amount.decimal(assetInfo: chainAsset.assetDisplayInfo)
        return formatter.stringFromDecimal(decimalAmount) ?? ""
    }
}

private extension AssetDetailsPresenter {
    func provideCoinageBreakdown() {
        func formatted(from decimal: Decimal, as assetInfo: AssetBalanceDisplayInfo) -> String {
            let balanceViewModelFactory = PrimitiveBalanceViewModelFactory(
                targetAssetInfo: assetInfo,
                formatterFactory: balanceFormatterFactory
            )
            return balanceViewModelFactory.balanceFromPrice(
                decimal,
                priceData: price
            )
            .value(for: .current)
            .amount
        }

        let amounts = coinageAmounts ?? .zero
        // Only the headline names the asset after its figure; the run markers set an amount beside
        // a label on one small line, where the headline above has already said what is counted.
        let figureInfo = chainAsset.asset.digitalDollarFigureDisplayInfo
        let strip = CoinageBreakdownFactory.stripCoins(CoinageBreakdownFactory.rows(from: holdings))

        let breakdown = CoinageBalanceBreakdownViewModel(
            totalBalance: formatted(from: amounts.total, as: figureInfo),
            availableNowBalance: formatted(from: amounts.availableNow, as: figureInfo),
            gainingPrivacyBalance: formatted(from: amounts.gainingPrivacy, as: figureInfo),
            symbol: chainAsset.asset.digitalDollarDisplayInfo.symbol,
            strip: strip
        )
        view?.didReceive(coinageBreakdown: breakdown)
    }
}

private extension AssetDetailsPresenter {
    func openRampProduct(_ action: RampAction) {
        view?.didReceive(rampLoading: action, isLoading: true)

        interactor?.openRampProduct(action)
    }
}
