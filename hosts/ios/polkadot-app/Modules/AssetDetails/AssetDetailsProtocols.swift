import Foundation
import PolkadotUI
import UIKit
import Coinage
import UIKitExt
import ChainRegistry
import Products

protocol AssetDetailsViewProtocol: ControllerBackedProtocol {
    func didSetCards(viewModels: [WalletCardCreateViewModel])
    func didReceiveData(viewModel: WalletCardDataViewModel, index: Int)
    func didReceive(readyAmount: BalanceViewModelProtocol?)
    func didReceive(paymentAsset: PaymentAssetViewModelProtocol)
    func didReceive(fundingStates: [AssetFundingStatusView.FundingState])
    func didReceive(isRecoveryInProgress: Bool)
    func didReceive(isAccountBackupPending: Bool)
    func didShowBackupNotification()
    func didHideBackupNotification()

    func didReceive(rampLoading action: RampAction, isLoading: Bool)

    func didReceive(coinageBreakdown: CoinageBalanceBreakdownViewModel)
    #if TESTNET_FEATURE
        func didReceive(testnetTopUpLoading: Bool)
    #endif
}

@MainActor
protocol AssetDetailsPresenterProtocol: AnyObject {
    func setup()
    func onSendMoney()
    func onAddMoney()
    func onFundingCompletedAction()
    func onFundingFailedAction()
    func onBackupSync()
    func onBackupCancel()
    func onBackupWhyUpdate()
    func onTopUp()
    func onWithdraw()

    #if TESTNET_FEATURE
        func onTestnetTopUp()
    #endif
}

protocol AssetDetailsInteractorInputProtocol: AnyObject {
    func setup()
    func removeCompletedFiatOnrampTransactions()
    func removeFailedFiatOnrampTransactions()
    func triggerSync()
    func cancelBackupNotification()

    func openRampProduct(_ action: RampAction)

    #if TESTNET_FEATURE
        func topUp()
    #endif
}

@MainActor
protocol AssetDetailsInteractorOutputProtocol: AnyObject {
    func didReceive(balance: Decimal)

    func didReceive(price: PriceData?)
    func didReceive(fiatOnrampStatuses: Set<FiatOnrampTransactionStatusPayload>)
    func didReceive(isRecoveryInProgress: Bool)
    func didReceive(isAccountBackupPending: Bool)
    func didReceive(showsRecoveredBalance: Bool)

    func didResolveRampProduct(_ action: RampAction, result: Result<ProductPage, Error>)

    /// One call, because the presenter rebuilds the whole breakdown on receipt: delivering the
    /// figures and the holdings separately would render the new totals beside the previous
    /// holdings, which is the mismatch `CoinageSummary` exists to prevent.
    func didReceive(coinageAmounts: CoinageAmounts, holdings: CoinageHoldings)
    #if TESTNET_FEATURE
        func didCompleteTopUp(_ result: Result<Void, Error>)
    #endif
}

@MainActor
protocol AssetDetailsWireframeProtocol: AlertPresentable, ErrorPresentable, BackupSyncPresentable {
    func showTransfer(from view: ControllerBackedProtocol?, chainAsset: ChainAsset)

    func showAddTokens(from view: ControllerBackedProtocol?)

    func showProduct(page: ProductPage)
}
