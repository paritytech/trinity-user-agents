import Coinage
import Foundation
import Revive
import Operation_iOS
import KeyDerivation
import Keystore_iOS
import FoundationExt
import SubstrateOperation
import ChainRegistry
import BackgroundExecution
import DurableTransactions
import ExtrinsicService
import Individuality

extension ServiceCoordinator {
    struct CoinageServices {
        let coinageService: CoinageServicing
        /// The one durable transaction engine every domain shares; the coordinator starts and stops it.
        let durableTransactionEngine: DurableTxServices
        let transferMonitor: CoinageTransferMonitoring
        let w3sPaymentTracking: W3sPaymentTracking
        let backupSyncService: CoinageBackupSyncServicing
    }

    /// `allowanceManager` is the coordinator's PGAS manager: the data store account is topped up through
    /// the same one as everything else.
    static func createCoinageServices(allowanceManager: AllowanceManaging) -> CoinageServices? {
        let databaseFactory = CoinageDatabaseDependencyFactory(storageFacade: UserDataStorageFacade.shared)

        let externalPaymentStore = ExternalPaymentCoreDataStore(
            storageFacade: UserDataStorageFacade.shared
        )

        let incomingPaymentStore = IncomingPaymentCoreDataStore(
            storageFacade: UserDataStorageFacade.shared
        )

        let chainViewFactory = PinnedChainViewFactory(
            chainResource: ChainRegistryFacade.sharedRegistry,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: Logger.shared
        )
        let durableEngine = createDurableTransactionEngine(chainViewFactory: chainViewFactory)

        guard let coinageService = createCoinageService(
            allowanceManager: allowanceManager,
            databaseFactory: databaseFactory,
            externalPaymentStore: externalPaymentStore,
            incomingPaymentStore: incomingPaymentStore,
            durable: durableEngine,
            chainViewFactory: chainViewFactory
        ) else {
            return nil
        }

        let transferMonitor = CoinageTransferMonitor(
            claimCoinsService: coinageService.claimCoinsService,
            transferStatusService: coinageService.transferStatusService,
            denominationContext: { try await coinageService.denominationContext() },
            transferStateStore: TransferStateCoreDataStore(storageFacade: UserDataStorageFacade.shared),
            storageFacade: UserDataStorageFacade.shared
        )

        let backupSyncService = CoinageBackupSyncService(
            coinageService: coinageService,
            storageFacade: UserDataStorageFacade.shared
        )

        return CoinageServices(
            coinageService: coinageService,
            durableTransactionEngine: durableEngine,
            transferMonitor: transferMonitor,
            w3sPaymentTracking: createW3sPaymentTracking(coinageService: coinageService),
            backupSyncService: backupSyncService
        )
    }

    private static func createW3sPaymentTracking(coinageService: CoinageServicing) -> W3sPaymentTracking {
        W3sPaymentTrackingService(
            historyStore: W3sPaymentHistoryCoreDataStore(
                storageFacade: UserDataStorageFacade.shared
            ),
            sendVerifier: coinageService.ongoingTransferService,
            blockInfoProvider: BlockInfoProvider(
                chainRegistry: ChainRegistryFacade.sharedRegistry,
                operationQueue: OperationManagerFacade.sharedDefaultQueue,
                chainId: AppConfig.Assets.mainAsset.chainId
            )
        )
    }
}

// MARK: - CoinageService Creation

private extension ServiceCoordinator {
    static func createCoinageService(
        allowanceManager: AllowanceManaging,
        databaseFactory: DatabaseDependencyFactoring,
        externalPaymentStore: ExternalPaymentStoring,
        incomingPaymentStore: IncomingPaymentStoring,
        durable: DurableTxServices,
        chainViewFactory: any PinnedChainViewFactoryProtocol
    ) -> CoinageService? {
        let logger = Logger.shared
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let coinageChainId = AppConfig.Assets.mainAsset.chainId
        let operationQueue = OperationManagerFacade.sharedDefaultQueue

        guard let chain = chainRegistry.getChain(for: coinageChainId) else {
            logger.error("Failed to get chain for coinageChainId: \(coinageChainId)")
            return nil
        }

        let vrfRepo: BandersnatchManagerRepositoryProtocol = .shared

        guard
            let connection = chainRegistry.getConnection(for: coinageChainId),
            let runtimeProvider = chainRegistry.getRuntimeProvider(for: coinageChainId),
            let fullPersonKeyManager = try? vrfRepo.fullPerson(),
            let lightPersonKeyManager = try? vrfRepo.litePerson()
        else {
            logger.error("Failed to get connection/runtime/personhood keys for coinage")
            return nil
        }

        let voucherKeypairFactory = VoucherKeypairFactory(
            entropyManager: RootEntropyManager.shared
        )

        let consumedTokenChecker = ConsumedTokenChecker(
            operationQueue: operationQueue,
            connection: connection,
            runtimeCodingService: runtimeProvider
        )
        let viewFunctionFetcher = ViewFunctionFetcher(
            executor: ViewFunctionExecutor(
                chainRegistry: chainRegistry,
                operationQueue: operationQueue
            ),
            chainId: coinageChainId
        )
        let unloadTokenResolver = UnloadTokenResolver(
            runtimeCodingService: runtimeProvider,
            viewFunctionFetcher: viewFunctionFetcher,
            consumedTokenChecker: consumedTokenChecker
        )

        let coinageOriginFactory = CoinageOriginFactory(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            chain: chain,
            voucherKeyFactory: voucherKeypairFactory,
            fullPersonKeyManager: fullPersonKeyManager,
            lightPersonKeyManager: lightPersonKeyManager,
            unloadTokenResolver: unloadTokenResolver,
            connection: connection,
            runtimeCodingService: runtimeProvider,
            logger: logger
        )

        let extrinsicMonitorFacade = ExtrinsicSubmissionMonitorFacade(
            chainRegistry: chainRegistry,
            substrateStorageFacade: SubstrateDataStorageFacade.shared,
            operationQueue: operationQueue
        )
        guard let monitorFactory = try? extrinsicMonitorFacade.createMonitorFactory(
            chain: chain
        ) else {
            logger.error("Failed to create extrinsic monitor factory")
            return nil
        }

        // Coinage registers its oracle with the shared engine inside `CoinageService.make`; the
        // coordinator owns the engine's lifecycle.
        let assetLedger = CoinageAssetLedgerCoreData(storageFacade: UserDataStorageFacade.shared)

        let incomingPaymentSecretStore = IncomingPaymentKeychainSecretStore(keychain: Keychain(), logger: logger)
        let incomingPaymentAcknowledger = TopUpAcknowledgementPresenter()

        guard let installation = createInstallationDependency(
            allowanceManager: allowanceManager,
            chainRegistry: chainRegistry,
            extrinsicMonitorFacade: extrinsicMonitorFacade,
            logger: logger
        ) else {
            return nil
        }

        return CoinageService.make(
            chainResource: chainRegistry,
            chain: chain,
            instanceId: AppConfig.Coinage.instanceId,
            databaseFactory: databaseFactory,
            originFactory: coinageOriginFactory,
            extrinsicMonitorFactory: monitorFactory,
            durable: durable,
            chainViewFactory: chainViewFactory,
            assetLedger: assetLedger,
            rootEntropyManager: RootEntropyManager.shared,
            applicationStateStreamFactory: ApplicationStateStreamFactory(),
            externalPaymentStore: externalPaymentStore,
            incomingPaymentStore: incomingPaymentStore,
            incomingPaymentSecretStore: incomingPaymentSecretStore,
            incomingPaymentAcknowledger: incomingPaymentAcknowledger,
            backgroundExecutor: ConnectionRetainingExecutor(provider: chainRegistry),
            recyclingStrategySettings: CoinageRecyclingStrategyStore.shared,
            personOriginProvider: coinageOriginFactory.personOriginProvider,
            viewFunctionFetcher: viewFunctionFetcher,
            installation: installation,
            logger: logger
        )
    }

    /// Registration and recovery of installations run against the `AccountDataStore` contract on
    /// Asset Hub, paid in PGAS by the seed's `//datastore` account.
    static func createInstallationDependency(
        allowanceManager: AllowanceManaging,
        chainRegistry: ChainRegistryProtocol,
        extrinsicMonitorFacade: ExtrinsicSubmissionMonitorFacade,
        logger: LoggerProtocol
    ) -> CoinageInstallationDependency? {
        let assetHubChainId = AppConfig.Chains.assethubChain

        guard
            let assetHub = chainRegistry.getChain(for: assetHubChainId),
            let runtimeProvider = chainRegistry.getRuntimeProvider(for: assetHubChainId)
        else {
            logger.error("Installation registration: Asset Hub \(assetHubChainId) is not in the chain registry")
            return nil
        }
        let pgasProvisioner = PGASAccountProvisioner.forDataStoreAccount(
            allowanceManager: allowanceManager,
            chainRegistry: chainRegistry
        )

        let currentInstallationStore = CoinageCurrentInstallationStore(
            repository: CoinageCurrentInstallationCoreDataRepository(storageFacade: UserDataStorageFacade.shared),
            keystore: Keychain(),
            tags: CoinageInstallationKeychainTags()
        )

        return CoinageInstallationDependency(
            currentInstallationStore: currentInstallationStore,
            chainId: assetHubChainId,
            runtimeService: runtimeProvider,
            reviveApi: ReviveContractApi(
                chainId: assetHubChainId,
                chainResource: chainRegistry,
                operationQueue: OperationManagerFacade.sharedDefaultQueue
            ),
            configProvider: AccountDataStoreConfigProvider(),
            pgasProvisioner: pgasProvisioner,
            feeEstimator: CoinageRegistrationFeeEstimator(
                chain: assetHub,
                extrinsicFacade: extrinsicMonitorFacade,
                versionProvider: ExtrinsicVersionProvider()
            )
        )
    }
}
