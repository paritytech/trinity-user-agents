import Foundation
import PolkadotUI
import UIKit
import Coinage
import UIKitExt
import ChainRegistry
import TrUAPIHost

protocol AssetDetailsViewProtocol: ControllerBackedProtocol {
    func didSetCards(viewModels: [WalletCardCreateViewModel])
    func didReceiveData(viewModel: WalletCardDataViewModel, index: Int)
    func didReceive(readyAmount: BalanceViewModelProtocol?)
    func didReceive(paymentAsset: PaymentAssetViewModelProtocol)
    func didReceive(isRecoveryInProgress: Bool)
    func didReceive(showsRestoredBalanceHint: Bool)

    func didReceive(fundingLoading direction: FundingDirection, isLoading: Bool)

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
    func onOpenBackupSettings()
    func onTopUp()
    func onHistory()

    #if TESTNET_FEATURE
        func onTestnetTopUp()
    #endif
}

protocol AssetDetailsInteractorInputProtocol: AnyObject {
    func setup()

    func openFunding(_ direction: FundingDirection)

    #if TESTNET_FEATURE
        func topUp()
    #endif
}

@MainActor
protocol AssetDetailsInteractorOutputProtocol: AnyObject {
    func didReceive(balance: Decimal)

    func didReceive(price: PriceData?)
    func didReceive(isRecoveryInProgress: Bool)
    func didReceive(showsRecoveredBalance: Bool)

    /// The session the user started, or `nil` when they closed the overlay.
    func didCompleteFunding(_ direction: FundingDirection, result: Result<String?, Error>)

    /// One call, because the presenter rebuilds the whole breakdown on receipt: delivering the
    /// figures and the holdings separately would render the new totals beside the previous
    /// holdings, which is the mismatch `CoinageSummary` exists to prevent.
    func didReceive(
        coinageAmounts: CoinageAmounts,
        holdings: CoinageHoldings,
        denominations: DenominationBreakdownContext
    )
    #if TESTNET_FEATURE
        func didCompleteTopUp(_ result: Result<Void, Error>)
    #endif
}

@MainActor
protocol AssetDetailsWireframeProtocol: AlertPresentable, ErrorPresentable {
    func showTransfer(from view: ControllerBackedProtocol?, chainAsset: ChainAsset)

    func showAddTokens(from view: ControllerBackedProtocol?)

    func showFundingHistory()

    func showBackupSettings()
}
