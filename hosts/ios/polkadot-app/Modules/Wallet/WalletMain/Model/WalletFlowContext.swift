import Foundation
import SubstrateSdk
import Coinage
import ChainRegistry
import Products

protocol WalletFlowContextProtocol {
    var depositService: DepositServiceProtocol { get }
    var fiatOnrampService: FiatOnrampServicing { get }
    var fiatOnrampTrackingService: FiatOnrampTrackingServiceProtocol { get }
    var coinageService: CoinageServicing { get }
    var coinageBackupSyncService: CoinageBackupSyncServicing { get }
    var networkStatusService: NetworkStatusProviding { get }
    var flowState: SPAFlowState { get }
}

final class WalletFlowContext: WalletFlowContextProtocol {
    let depositService: DepositServiceProtocol
    let fiatOnrampService: FiatOnrampServicing
    let fiatOnrampTrackingService: FiatOnrampTrackingServiceProtocol
    let coinageService: CoinageServicing
    let coinageBackupSyncService: CoinageBackupSyncServicing
    let networkStatusService: NetworkStatusProviding
    let flowState: SPAFlowState

    init(
        depositService: DepositServiceProtocol,
        fiatOnrampService: FiatOnrampServicing,
        fiatOnrampTrackingService: FiatOnrampTrackingServiceProtocol,
        coinageService: CoinageServicing,
        coinageBackupSyncService: CoinageBackupSyncServicing,
        networkStatusService: NetworkStatusProviding,
        flowState: SPAFlowState
    ) {
        self.depositService = depositService
        self.fiatOnrampService = fiatOnrampService
        self.fiatOnrampTrackingService = fiatOnrampTrackingService
        self.coinageService = coinageService
        self.coinageBackupSyncService = coinageBackupSyncService
        self.networkStatusService = networkStatusService
        self.flowState = flowState
    }
}
