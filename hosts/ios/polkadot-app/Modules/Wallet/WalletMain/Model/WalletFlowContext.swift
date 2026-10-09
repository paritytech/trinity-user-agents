import Foundation
import SubstrateSdk
import Coinage
import ChainRegistry
import Products

protocol WalletFlowContextProtocol {
    var depositService: DepositServiceProtocol { get }
    var coinageService: CoinageServicing { get }
    var coinageBackupSyncService: CoinageBackupSyncServicing { get }
    var personDataStore: DetermineStatePersonDataStore { get }
    var networkStatusService: NetworkStatusProviding { get }
    var flowState: SPAFlowState { get }
}

final class WalletFlowContext: WalletFlowContextProtocol {
    let depositService: DepositServiceProtocol
    let coinageService: CoinageServicing
    let coinageBackupSyncService: CoinageBackupSyncServicing
    let personDataStore: DetermineStatePersonDataStore
    let networkStatusService: NetworkStatusProviding
    let flowState: SPAFlowState

    init(
        depositService: DepositServiceProtocol,
        coinageService: CoinageServicing,
        coinageBackupSyncService: CoinageBackupSyncServicing,
        personDataStore: DetermineStatePersonDataStore,
        networkStatusService: NetworkStatusProviding,
        flowState: SPAFlowState
    ) {
        self.depositService = depositService
        self.coinageService = coinageService
        self.coinageBackupSyncService = coinageBackupSyncService
        self.personDataStore = personDataStore
        self.networkStatusService = networkStatusService
        self.flowState = flowState
    }
}
