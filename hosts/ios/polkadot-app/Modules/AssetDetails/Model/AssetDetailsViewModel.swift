import Observation
import PolkadotUI
import Combine
import SubstrateSdk
import Foundation

struct CoinageBalanceBreakdownViewModel {
    /// Fiat-signed amounts ("$30"): the headline carries ``symbol`` once, in lighter type, and
    /// the two figures below it are read against that.
    let totalBalance: String
    let availableNowBalance: String
    let gainingPrivacyBalance: String
    let symbol: String
    /// Every holding, in display order, drawn as coins. Clearing leads.
    let strip: [CoinageScene.Coin]
}

protocol AssetDetailsViewModelProtocol: Observation.Observable {
    var balanceCardModel: AssetDetailsBalanceCard.ViewModel? { get set }
    var showsBackupNotification: Bool { get set }
    /// This installation's on-chain registration has not landed in the expected time (D4).
    var showsAccountBackupPending: Bool { get set }
    var fundingStates: [AssetFundingStatusView.FundingState] { get set }
    var isFundingExpanded: Bool { get set }
    var isUpdating: Bool { get set }

    var onAddMoney: (() -> Void)? { get set }
    var onSendMoney: (() -> Void)? { get set }
    var onFundingCompleted: (() -> Void)? { get set }
    var onFundingFailed: (() -> Void)? { get set }
    var onBackupSync: (() -> Void)? { get set }
    var onBackupCancel: (() -> Void)? { get set }
    var onBackupWhyUpdate: (() -> Void)? { get set }

    var isTopUpInProgress: Bool { get set }
    var onTopUp: (() -> Void)? { get set }
    var isWithdrawInProgress: Bool { get set }
    var onWithdraw: (() -> Void)? { get set }

    var coinageBreakdown: CoinageBalanceBreakdownViewModel? { get set }
    #if TESTNET_FEATURE
        var isTestnetTopUpInProgress: Bool { get set }
        var onTestnetTopUp: (() -> Void)? { get set }
    #endif
}

@Observable
class AssetDetailsViewModel: AssetDetailsViewModelProtocol {
    var balanceCardModel: AssetDetailsBalanceCard.ViewModel?
    var showsBackupNotification: Bool = false
    var showsAccountBackupPending: Bool = false
    var fundingStates: [AssetFundingStatusView.FundingState] = []
    var isFundingExpanded: Bool = false
    var isUpdating: Bool = false

    var onAddMoney: (() -> Void)?
    var onSendMoney: (() -> Void)?
    var onFundingCompleted: (() -> Void)?
    var onFundingFailed: (() -> Void)?
    var onBackupSync: (() -> Void)?
    var onBackupCancel: (() -> Void)?
    var onBackupWhyUpdate: (() -> Void)?

    var isTopUpInProgress: Bool = false
    var onTopUp: (() -> Void)?
    var isWithdrawInProgress: Bool = false
    var onWithdraw: (() -> Void)?

    var coinageBreakdown: CoinageBalanceBreakdownViewModel?
    #if TESTNET_FEATURE
        var isTestnetTopUpInProgress: Bool = false
        var onTestnetTopUp: (() -> Void)?
    #endif
}
