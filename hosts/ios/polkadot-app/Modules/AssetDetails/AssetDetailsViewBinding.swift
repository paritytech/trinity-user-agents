import Foundation
import PolkadotUI
import SwiftUI
import UIKit
import UIKitExt

final class AssetDetailsViewBinding: AssetDetailsViewProtocol {
    let viewModel: AssetDetailsViewModel
    weak var navigationHost: ControllerBackedProtocol?
    var animatesBackupNotificationUpdates = true

    private var cardCreateModel: WalletCardCreateViewModel?
    private var amount: String?
    private var readyAmountString: String?
    private var assetLogo: UIImage?
    private var assetSymbol: String?
    private var paymentAssetViewModel: PaymentAssetViewModelProtocol?

    init(viewModel: AssetDetailsViewModel) {
        self.viewModel = viewModel
    }

    deinit {
        paymentAssetViewModel?.cancel()
    }

    var isSetup: Bool {
        navigationHost?.isSetup ?? false
    }

    var controller: UIViewController {
        guard let controller = navigationHost?.controller else {
            assertionFailure("AssetDetailsViewBinding requires a navigation host before presenting UI")
            return UIViewController()
        }

        return controller
    }

    func bind(to presenter: AssetDetailsPresenterProtocol) {
        viewModel.onSendMoney = { [weak presenter] in
            presenter?.onSendMoney()
        }

        viewModel.onAddMoney = { [weak presenter] in
            presenter?.onAddMoney()
        }

        viewModel.onFundingCompleted = { [weak presenter] in
            presenter?.onFundingCompletedAction()
        }

        viewModel.onFundingFailed = { [weak presenter] in
            presenter?.onFundingFailedAction()
        }

        viewModel.onBackupSync = { [weak presenter] in
            presenter?.onBackupSync()
        }

        viewModel.onBackupCancel = { [weak presenter] in
            presenter?.onBackupCancel()
        }

        viewModel.onBackupWhyUpdate = { [weak presenter] in
            presenter?.onBackupWhyUpdate()
        }

        viewModel.onTopUp = { [weak presenter] in
            presenter?.onTopUp()
        }

        viewModel.onWithdraw = { [weak presenter] in
            presenter?.onWithdraw()
        }

        #if TESTNET_FEATURE
            viewModel.onTestnetTopUp = { [weak presenter] in
                presenter?.onTestnetTopUp()
            }
        #endif
    }

    func didSetCards(viewModels: [WalletCardCreateViewModel]) {
        cardCreateModel = viewModels.first
        emitCardUpdate()
    }

    func didReceiveData(viewModel: WalletCardDataViewModel, index _: Int) {
        switch viewModel {
        case let .token(balance):
            amount = balance.amount
        }

        emitCardUpdate()
    }

    func didReceive(readyAmount: BalanceViewModelProtocol?) {
        defer {
            emitCardUpdate()
        }

        guard let readyAmount else {
            readyAmountString = nil
            return
        }

        readyAmountString = readyAmount.amount
    }

    func didReceive(paymentAsset: PaymentAssetViewModelProtocol) {
        paymentAssetViewModel?.cancel()
        paymentAssetViewModel = paymentAsset

        paymentAsset.bind { [weak self] brand in
            self?.apply(brand)
        }
    }

    private func apply(_ brand: PaymentAssetBrand) {
        assetLogo = brand.squareIcon
        assetSymbol = brand.symbol

        guard viewModel.balanceCardModel != nil else { return }
        emitCardUpdate()
    }

    func didReceive(coinageBreakdown: CoinageBalanceBreakdownViewModel) {
        viewModel.coinageBreakdown = coinageBreakdown
    }

    #if TESTNET_FEATURE
        func didReceive(testnetTopUpLoading: Bool) {
            viewModel.isTestnetTopUpInProgress = testnetTopUpLoading
        }
    #endif

    func didReceive(fundingStates: [AssetFundingStatusView.FundingState]) {
        viewModel.fundingStates = fundingStates
    }

    func didReceive(isRecoveryInProgress: Bool) {
        viewModel.isUpdating = isRecoveryInProgress
    }

    func didReceive(isAccountBackupPending: Bool) {
        withAnimation(.easeInOut) {
            viewModel.showsAccountBackupPending = isAccountBackupPending
        }
    }

    func didShowBackupNotification() {
        guard animatesBackupNotificationUpdates else {
            viewModel.showsBackupNotification = true
            return
        }

        withAnimation(.easeInOut) {
            viewModel.showsBackupNotification = true
        }
    }

    func didHideBackupNotification() {
        withAnimation(.easeInOut) {
            viewModel.showsBackupNotification = false
        }
    }

    func didReceive(rampLoading action: RampAction, isLoading: Bool) {
        switch action {
        case .topUp: viewModel.isTopUpInProgress = isLoading
        case .withdraw: viewModel.isWithdrawInProgress = isLoading
        }
    }

    private func emitCardUpdate() {
        viewModel.balanceCardModel = .init(
            balance: amount,
            readyBalance: readyAmountString,
            logo: assetLogo,
            symbol: assetSymbol
        )
    }
}
