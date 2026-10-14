import DurableTransactions
import Foundation
import Individuality
import Revive
import SubstrateSdk

/// The app-side pieces installation allocation, registration and recovery run on: the store of the
/// current installation and its counters, pallet-revive and fee estimation on the chain the `AccountDataStore`
/// contract lives on, and PGAS for the data store account.
public struct CoinageInstallationDependency {
    public let currentInstallationStore: any CoinageCurrentInstallationStoring
    public let chainId: ChainId
    public let runtimeService: any RuntimeCodingServiceProtocol
    public let reviveApi: any ReviveContractApiProtocol
    public let configProvider: any AccountDataStoreConfigProviding
    public let pgasProvisioner: any PGASAccountProvisioning
    public let feeEstimator: any RegistrationFeeEstimating

    public init(
        currentInstallationStore: any CoinageCurrentInstallationStoring,
        chainId: ChainId,
        runtimeService: any RuntimeCodingServiceProtocol,
        reviveApi: any ReviveContractApiProtocol,
        configProvider: any AccountDataStoreConfigProviding,
        pgasProvisioner: any PGASAccountProvisioning,
        feeEstimator: any RegistrationFeeEstimating
    ) {
        self.currentInstallationStore = currentInstallationStore
        self.chainId = chainId
        self.runtimeService = runtimeService
        self.reviveApi = reviveApi
        self.configProvider = configProvider
        self.pgasProvisioner = pgasProvisioner
        self.feeEstimator = feeEstimator
    }
}
