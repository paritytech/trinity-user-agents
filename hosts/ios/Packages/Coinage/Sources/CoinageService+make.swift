import Foundation
import ExtrinsicService
import KeyDerivation
import NovaCrypto
import Operation_iOS
import SDKLogger
import ChainStore
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateOperation
import FoundationExt
import BackgroundExecution
import Individuality
import DurableTransactions
import Revive

public extension CoinageService {
    /// Creates a CoinageService instance.
    ///
    /// - Parameters:
    ///   - chainResource: Chain resource for RPC connections
    ///   - chain: The chain configuration
    ///   - instanceId: Coinage pallet instance the app operates on (from remote config)
    ///   - databaseFactory: Factory for creating database repositories
    ///   - originFactory: Factory for creating extrinsic origins (app-side implementation)
    ///   - extrinsicMonitorFactory: Factory for extrinsic submission monitoring
    ///   - durable: The shared durability layer; coinage registers its oracle and its policies with it
    ///   - chainViewFactory: Pinned chain views for reads outside the engine
    ///   - assetLedger: Coinage's half of the ledger (asset rows, handoff marks)
    ///   - rootEntropyManager: Manager for root entropy (key derivation)
    ///   - logger: Logger for diagnostic output
    ///   - installation: What registering this installation and recovering the previous ones needs
    /// - Returns: A configured CoinageServicing instance
    // swiftlint:disable:next function_body_length
    static func make(
        chainResource: ChainResourceProtocol,
        chain: ChainProtocol,
        instanceId: CoinageInstanceId,
        databaseFactory: DatabaseDependencyFactoring,
        originFactory: OriginCreating,
        extrinsicMonitorFactory: ExtrinsicSubmitMonitorFactoryProtocol,
        durable: DurableTxServices,
        chainViewFactory: any PinnedChainViewFactoryProtocol,
        assetLedger: any CoinageAssetLedgerProtocol,
        rootEntropyManager: RootEntropyManaging,
        applicationStateStreamFactory: ApplicationStateStreamFactory,
        externalPaymentStore: ExternalPaymentStoring,
        incomingPaymentStore: IncomingPaymentStoring,
        incomingPaymentSecretStore: IncomingPaymentSecretStoring,
        incomingPaymentAcknowledger: IncomingPaymentAcknowledging,
        backgroundExecutor: any BackgroundExecuting,
        recyclingStrategySettings: any CoinageRecyclingStrategyProviding,
        personOriginProvider: any OriginPersonProviding,
        viewFunctionFetcher: any ViewFunctionFetching,
        installation: CoinageInstallationDependency,
        logger: SDKLoggerProtocol
    ) -> CoinageService {
        let operationQueue = OperationQueue()

        guard let connection = chainResource.getRpcConnection(for: chain.chainId) else {
            fatalError("Failed to get RPC connection for chain \(chain.chainId)")
        }
        guard let runtimeService = chainResource.getRuntimeCodingService(for: chain.chainId) else {
            fatalError("Failed to get runtime service for chain \(chain.chainId)")
        }

        let coinRepository = databaseFactory.makeCoinRepository()
        let trackedVoucherRepository = databaseFactory.makeTrackedVoucherRepository()
        let voucherRepository = databaseFactory.makeVoucherRepository()

        let installationRepository = databaseFactory.makeInstallationRepository()
        let currentInstallationStore = installation.currentInstallationStore
        let coinKeypairFactory = CoinKeypairFactory(entropyManager: rootEntropyManager)
        let voucherKeypairFactory = VoucherKeypairFactory(entropyManager: rootEntropyManager)
        // One allocator per Coinage instance: each serialises its own reserve-then-save.
        let coinAllocator = CoinAllocator(
            installationStore: currentInstallationStore,
            coinRepository: coinRepository,
            keyFactory: coinKeypairFactory
        )
        let voucherAllocator = VoucherAllocator(
            installationStore: currentInstallationStore,
            delayProvider: VoucherDelayProvider(),
            voucherRepository: voucherRepository,
            keyFactory: voucherKeypairFactory
        )

        // One minter per Coinage instance wraps both allocators; every mint site goes through it.
        let coinageMinter = CoinageMinter(coinAllocator: coinAllocator, voucherAllocator: voucherAllocator)

        let coinService = CoinService(databaseFactory: databaseFactory)

        let storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )

        let contextLoader = DenominationContextLoader(
            instanceId: instanceId,
            connection: connection,
            storageRequestFactory: storageRequestFactory,
            runtimeService: runtimeService
        )

        let readinessLoader = RecyclerReadinessLoader(
            instanceId: instanceId,
            connection: connection,
            runtimeCodingService: runtimeService,
            operationQueue: operationQueue
        )

        let coinSelector = CoinSelector()

        let blockNumberProvider = BlockInfoProvider(
            chainRegistry: chainResource,
            operationQueue: operationQueue,
            chainId: chain.chainId
        )

        let coinOnChainQuery = CoinOnChainQueryService(
            connection: connection,
            runtimeService: runtimeService,
            storageRequestFactory: storageRequestFactory
        )

        let voucherOnChainQuery = VoucherOnChainQueryService(
            instanceId: instanceId,
            connection: connection,
            runtimeService: runtimeService,
            storageRequestFactory: storageRequestFactory,
            publicKeyProvider: { try voucherKeypairFactory.derivePublicKey(index: $0) },
            aliasProvider: { try voucherKeypairFactory.alias(for: $0) }
        )

        // Coinage's oracle answers the engine's two questions from its asset rows and its own chain
        // reads; the engine owns the ledger row, the submission watch and recovery.
        let stateReader = CoinageStateReader(coinQuery: coinOnChainQuery, voucherQuery: voucherOnChainQuery)
        durable.oracles.register(
            CoinageResourceOracle(chainId: chain.chainId, ledger: assetLedger, reader: stateReader),
            for: .coinage
        )

        let txService = CoinageTxService(engine: durable.txService, ledger: assetLedger, logger: logger)

        let voucherLoaderFactory = VoucherLoaderFactory(
            instanceId: instanceId,
            minter: coinageMinter,
            keypairFactory: voucherKeypairFactory,
            txService: txService,
            originCreating: originFactory,
            runtimeService: runtimeService,
            chain: chain,
            logger: logger
        )
        let voucherService = VoucherService(
            databaseFactory: databaseFactory,
            trackedVoucherRepository: trackedVoucherRepository,
            voucherLoaderFactory: voucherLoaderFactory
        )

        let consumedTokenChecker = ConsumedTokenChecker(
            operationQueue: operationQueue,
            connection: connection,
            runtimeCodingService: runtimeService
        )
        let dateProvider = NowDateProvider()
        let quotaTracker = UnloadQuotaTracker(
            runtimeCodingService: runtimeService,
            consumedTokenChecker: consumedTokenChecker,
            personOriginProvider: personOriginProvider,
            viewFunctionFetcher: viewFunctionFetcher,
            dateProvider: dateProvider
        )

        let planFactory = TransferPlanFactory(
            minter: coinageMinter,
            durability: txService,
            dateProvider: dateProvider,
            logger: logger
        )

        let memoBuilder = MemoBuilder(
            privateKeyDeriver: coinKeypairFactory
        )

        let senderService = TransferSenderService(
            coinSelector: coinSelector,
            planFactory: planFactory,
            memoBuilder: memoBuilder,
            recyclerLoader: readinessLoader,
            txService: txService,
            logger: logger
        )

        let dataStoreRepository = AccountDataStoreRepository(
            accountKeys: DataStoreAccountKeys(entropyManager: rootEntropyManager),
            reviveApi: installation.reviveApi,
            logger: logger
        )
        durable.oracles.register(
            CoinageInstallationRegistrationOracle.make(
                chainId: installation.chainId,
                dataStoreRepository: dataStoreRepository,
                logger: logger
            ),
            for: .coinageInstallation
        )
        let installationRegistrar = CoinageInstallationRegistrar(
            currentInstallationStore: currentInstallationStore,
            configProvider: installation.configProvider,
            engine: durable.txService,
            submitter: InstallationRegistrationSubmitter(
                dataStoreRepository: dataStoreRepository,
                reviveApi: installation.reviveApi,
                pgasProvisioner: installation.pgasProvisioner,
                feeEstimator: installation.feeEstimator,
                callArguments: RuntimeReviveCallArguments(runtimeService: installation.runtimeService),
                chainId: installation.chainId,
                originFactory: originFactory,
                engine: durable.txService,
                logger: logger
            ),
            backgroundExecutor: backgroundExecutor,
            logger: logger
        )

        let recoveryService = CoinageBackupRecoveryService(
            currentInstallationStore: currentInstallationStore,
            installationRepository: installationRepository,
            configProvider: installation.configProvider,
            dataStoreRepository: dataStoreRepository,
            scanner: InstallationAssetScanner(
                coinKeypairFactory: coinKeypairFactory,
                coinOnChainQuery: coinOnChainQuery,
                voucherOnChainQuery: voucherOnChainQuery,
                chainViewFactory: chainViewFactory,
                chainId: chain.chainId,
                logger: logger
            ),
            assetStore: RecoveredAssetStore(databaseFactory: databaseFactory),
            logger: logger
        )

        let transferSubmitter = CoinTransferSubmitter(
            originFactory: originFactory,
            extrinsicMonitor: extrinsicMonitorFactory
        )

        let claimSubmitter = CoinageClaimSubmitter(
            minter: coinageMinter,
            originFactory: originFactory,
            txService: txService,
            logger: logger
        )

        let claimCoinsService = ClaimCoinsService(
            txService: txService,
            coinOnChainQuery: coinOnChainQuery,
            claimSubmitter: claimSubmitter,
            snKeyFactory: SNKeyFactory(),
            coinService: coinService,
            logger: logger
        )

        // Coinage's three submission policies, registered before anything carrying one is submitted:
        // a row naming an unregistered policy is abandoned rather than left waiting for ever.
        CoinageSubmissionPolicies.register(
            into: durable.policies,
            using: CoinageSubmissionPolicies.Dependencies(
                chainId: chain.chainId,
                ledger: assetLedger,
                coinService: coinService,
                voucherService: voucherService,
                coinQuery: coinOnChainQuery,
                voucherSnapshots: { databaseFactory.makeTrackedVoucherSnapshotStream() },
                splitBuilder: SplitExtrinsicBuilder(
                    coinKeyFactory: coinKeypairFactory,
                    originFactory: originFactory,
                    factory: durable.factory,
                    chainId: chain.chainId
                ),
                unloadBuilder: UnloadExtrinsicBuilder(
                    instanceId: instanceId,
                    voucherKeyFactory: voucherKeypairFactory,
                    recyclerLoader: readinessLoader,
                    originFactory: originFactory,
                    blockInfoProvider: blockNumberProvider,
                    quotaTracker: quotaTracker,
                    factory: durable.factory,
                    chainId: chain.chainId,
                    logger: logger
                ),
                claimBuilder: ClaimExtrinsicBuilder(
                    originFactory: originFactory,
                    factory: durable.factory,
                    chainId: chain.chainId
                ),
                snKeyFactory: SNKeyFactory(),
                dateProvider: dateProvider,
                logger: logger
            )
        )

        let recipientService = TransferRecipientService(
            coinMinter: coinageMinter,
            coinKeyFactory: coinKeypairFactory,
            coinService: coinService,
            coinOnChainQuery: coinOnChainQuery,
            transferSubmitter: transferSubmitter,
            snKeyFactory: SNKeyFactory(),
            claimCoinsService: claimCoinsService,
            blockNumberProvider: blockNumberProvider,
            logger: logger
        )

        let coinStateSyncService = CoinStateSyncService(
            coinService: coinService,
            databaseFactory: databaseFactory,
            connection: connection,
            runtimeService: runtimeService,
            logger: logger
        )

        let ringCapacityProvider = RingCapacityProvider(
            instanceId: instanceId,
            operationQueue: operationQueue,
            connection: connection,
            runtimeCodingService: runtimeService
        )

        let voucherLocationService = VoucherLocationService(
            instanceId: instanceId,
            voucherRepository: voucherRepository,
            databaseFactory: databaseFactory,
            connection: connection,
            runtimeService: runtimeService,
            ringCapacityProvider: ringCapacityProvider,
            logger: logger
        )

        let recyclingService = CoinageRecyclingService(
            voucherMinter: coinageMinter,
            coinKeypairFactory: coinKeypairFactory,
            voucherKeypairFactory: voucherKeypairFactory,
            txService: txService,
            voucherService: voucherService,
            originFactory: originFactory,
            backgroundExecutor: backgroundExecutor,
            logger: logger
        )

        let recyclingStrategyResolver = RecyclingStrategyProvider(quotaTracker: quotaTracker)
        let preClassificator = CoinageAssetPreClassificator()

        let externalPaymentDependency = ExternalPaymentDependency(
            instanceId: instanceId,
            coinService: coinService,
            voucherService: voucherService,
            assetClassifier: ExternalPaymentAssetClassifier(
                settings: recyclingStrategySettings,
                strategyResolver: recyclingStrategyResolver,
                ringCapacityProvider: ringCapacityProvider,
                preClassificator: preClassificator,
                logger: logger
            ),
            recycler: recyclingService,
            voucherKeyFactory: voucherKeypairFactory,
            voucherMinter: coinageMinter,
            recyclerLoader: readinessLoader,
            extrinsicMonitor: extrinsicMonitorFactory,
            durability: txService,
            originFactory: originFactory,
            quotaTracker: quotaTracker,
            blockNumberProvider: blockNumberProvider,
            dateProvider: dateProvider
        )

        let externalPaymentService = ExternalPaymentService(
            store: externalPaymentStore,
            dependency: externalPaymentDependency,
            logger: logger
        )

        let transferStatusService = CoinageTransferStatusService(
            databaseFactory: databaseFactory,
            chainViewFactory: chainViewFactory,
            chainId: chain.chainId,
            coinOnChainQuery: coinOnChainQuery,
            snKeyFactory: SNKeyFactory(),
            logger: logger
        )

        let assetsTracking = AssetBalanceTracker(
            connection: connection,
            runtimeService: runtimeService,
            storageRequestFactory: storageRequestFactory,
            logger: logger
        )

        let claimAssetService = ClaimAssetService(
            assetsTracking: assetsTracking,
            voucherLoaderFactory: voucherLoaderFactory,
            voucherService: voucherService,
            txService: txService,
            logger: logger
        )

        let incomingPaymentSourceResolver = IncomingPaymentSourceResolver(
            entropyManager: rootEntropyManager,
            snKeyFactory: SNKeyFactory()
        )

        let incomingPaymentService = IncomingPaymentService(
            store: incomingPaymentStore,
            secretStore: incomingPaymentSecretStore,
            sourceResolver: incomingPaymentSourceResolver,
            paymentContext: IncomingPaymentContext(logger: logger),
            claimCoinsService: claimCoinsService,
            claimAssetService: claimAssetService,
            verdictResolver: CoinageGroupVerdictResolver(
                txService: txService,
                coinService: coinService,
                voucherService: voucherService
            ),
            acknowledger: incomingPaymentAcknowledger,
            instanceId: instanceId,
            logger: logger
        )

        let coinageService = CoinageService(
            coinService: coinService,
            voucherService: voucherService,
            coinKeypairFactory: coinKeypairFactory,
            senderService: senderService,
            ongoingTransferService: recipientService,
            txService: txService,
            claimCoinsService: claimCoinsService,
            transferStatusService: transferStatusService,
            externalPaymentService: externalPaymentService,
            contextLoader: contextLoader,
            coinStateSyncService: coinStateSyncService,
            voucherLocationService: voucherLocationService,
            recyclingService: recyclingService,
            recyclingStrategySettings: recyclingStrategySettings,
            recyclingStrategyResolver: recyclingStrategyResolver,
            ringCapacityProvider: ringCapacityProvider,
            preClassificator: preClassificator,
            applicationStateStreamFactory: applicationStateStreamFactory,
            databaseFactory: databaseFactory,
            recoveryService: recoveryService,
            installationRegistrar: installationRegistrar,
            incomingPaymentService: incomingPaymentService,
            logger: logger
        )

        return coinageService
    }
}

extension VoucherKeyDeriving {
    func alias(for index: CoinageKeyIndex) throws -> Data {
        try createKeyManager(index: index)
            .deriveAlias(for: UnloadTokenContextBuilder.recyclerAliasContext)
    }
}
