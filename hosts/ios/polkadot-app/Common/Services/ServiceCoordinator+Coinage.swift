import Coinage
import Foundation
import Operation_iOS
import ChainRegistry
import DurableTransactions

extension ServiceCoordinator {
    struct CoinageServices {
        let coinageService: CoinageServicing
        /// The one durable transaction engine every domain shares; the coordinator starts and stops it.
        let durableTransactionEngine: DurableTxServices
        let transferMonitor: CoinageTransferMonitoring
        let w3sPaymentTracking: W3sPaymentTracking
        let backupSyncService: CoinageBackupSyncServicing
    }

    /// The wallet is the core's coinage layer, reached through the shared runtime.
    static func createCoinageServices(
        runtimeProvider: TrUAPIHostRuntimeProviding,
        registry: AccountDataStoreConfigProviding
    ) -> CoinageServices? {
        let logger = Logger.shared
        let mainAssetId = AppConfig.Assets.mainAsset

        guard
            let chain = ChainRegistryFacade.sharedRegistry.getChain(for: mainAssetId.chainId),
            let asset = chain.asset(for: mainAssetId.assetId)
        else {
            logger.error("Failed to get the payment asset \(mainAssetId) for coinage")
            return nil
        }

        let chainViewFactory = PinnedChainViewFactory(
            chainResource: ChainRegistryFacade.sharedRegistry,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: logger
        )
        let durableEngine = createDurableTransactionEngine(chainViewFactory: chainViewFactory)

        let core = CoinageCore(precision: asset.decimalPrecision) {
            try await runtimeProvider.coinage()
        }
        let coinageService = CoinageService(
            core: core,
            strategySettings: CoinageRecyclingStrategyStore.shared,
            recoveryAcknowledgement: CoinageRecoveryAcknowledgementStore(),
            registryConfigured: registry.contractAddress != nil,
            topUpAcknowledger: TopUpAcknowledgementPresenter(),
            logger: logger
        )

        let transferMonitor = CoinageTransferMonitor(
            coinageService: coinageService,
            transferStateStore: TransferStateCoreDataStore(storageFacade: UserDataStorageFacade.shared),
            storageFacade: UserDataStorageFacade.shared
        )

        return CoinageServices(
            coinageService: coinageService,
            durableTransactionEngine: durableEngine,
            transferMonitor: transferMonitor,
            w3sPaymentTracking: createW3sPaymentTracking(coinageService: coinageService),
            backupSyncService: CoinageBackupSyncService(coinageService: coinageService)
        )
    }

    private static func createW3sPaymentTracking(coinageService: CoinageServicing) -> W3sPaymentTracking {
        W3sPaymentTrackingService(
            historyStore: W3sPaymentHistoryCoreDataStore(
                storageFacade: UserDataStorageFacade.shared
            ),
            coinageService: coinageService
        )
    }
}
