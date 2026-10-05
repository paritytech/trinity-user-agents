import Foundation
import Keystore_iOS
import Operation_iOS
import AssetExchange
import NovaCrypto
import CommonService
import ExtrinsicService
import KeyDerivation
import Coinage
import DurableTransactions
import Products
import SubstrateSdk
import SubstrateOperation
import SubstrateStorageQuery
import FoundationExt
import Individuality
import UniqueDevice
import ChainRegistry

protocol ServiceCoordinatorProtocol: ApplicationServiceProtocol {
    var depositService: DepositServiceProtocol { get }
    var fiatOnrampService: FiatOnrampServicing { get }
    var fiatOnrampTrackingService: FiatOnrampTrackingServiceProtocol { get }
    var fiatOnrampStorage: FiatOnrampStoring { get }
    var polkadotHandshakeService: PolkadotHandshakeServicing { get }
    var chatExtensionsRegistry: ChatExtensionsRegistering { get }
    var signInHostCoordinator: MessageExchangeSignInHostCoordinating { get }
    var chatCoordinator: MessageExchangeChatCoordinating { get }
    var callCoordinator: CallCoordinating { get }
    var attachmentUploadService: AttachmentUploadingServicing { get }
    var attachmentDownloadService: AttachmentDownloadingServicing { get }
    var audioSessionManager: AudioSessionManaging { get }
    var determineStateSyncService: DetermineStateSyncServicing { get }
    var personDataStore: DetermineStatePersonDataStore { get }
    var coinageService: CoinageServicing { get }
    var coinageBackupSyncService: CoinageBackupSyncServicing { get }
    var accountManager: ProductsAccountManaging { get }
    var allowanceManagerFacade: AllowanceManagerFacade { get }
    var turnService: TURNCredentialsProviding { get }
    var networkStatusService: NetworkStatusProviding { get }
    var chainStatusProvider: ChainStatusProviding { get }
    var truapiRuntimeProvider: TrUAPIHostRuntimeProviding { get }
}

final class ServiceCoordinator {
    let chatCoordinator: MessageExchangeChatCoordinating
    let depositService: DepositServiceProtocol
    let fiatOnrampService: FiatOnrampServicing
    let fiatOnrampTrackingService: FiatOnrampTrackingServiceProtocol
    let fiatOnrampStorage: FiatOnrampStoring
    let polkadotHandshakeService: PolkadotHandshakeServicing
    let signInHostCoordinator: MessageExchangeSignInHostCoordinating
    let chatExtensionsRegistry: ChatExtensionsRegistering
    /// Held so the product-worker ref counter and its operations stay alive while
    /// the main tab bar is, rather than living in a process-wide singleton.
    let productWorkerFacade: ProductWorkerFacade
    let callCoordinator: CallCoordinating
    let chatRequestCoordinator: ChatRequestCoordinatorServicing
    let attachmentUploadService: AttachmentUploadingServicing
    let attachmentDownloadService: AttachmentDownloadingServicing
    let coinageTransferMonitor: CoinageTransferMonitoring
    let durableTransactionEngine: DurableTxServices
    let w3sPaymentTracking: W3sPaymentTracking
    let audioSessionManager: AudioSessionManaging
    let determineStateSyncService: DetermineStateSyncServicing
    let personhoodBackgroundService: PersonhoodBackgroundService
    let personDataStore: DetermineStatePersonDataStore
    let coinageBackupSyncService: CoinageBackupSyncServicing
    let messageExpansionService: CompactedMessageExpansionServicing
    let notificationBadgeSyncService: NotificationBadgeSyncService
    let accountManager: ProductsAccountManaging
    let allowanceManagerFacade: AllowanceManagerFacade
    let allowanceRenewalService: AllowanceRenewalService
    let turnService: TURNCredentialsProviding
    let deviceSyncService: DeviceSyncServicing
    let networkStatusService: NetworkStatusProviding
    let chainStatusProvider: ChainStatusProviding
    let truapiRuntimeProvider: TrUAPIHostRuntimeProviding
    let tldProvider: DotNsTldProviding
    let logger: LoggerProtocol

    // Retained so the weakly-held dependency-locator entry stays alive for the product host.
    let paymentsSupport: PaymentsSupport

    var coinageService: CoinageServicing {
        paymentsSupport.coinageService
    }

    init(
        chatCoordinator: MessageExchangeChatCoordinating,
        depositService: DepositServiceProtocol,
        fiatOnrampService: FiatOnrampServicing,
        fiatOnrampTrackingService: FiatOnrampTrackingServiceProtocol,
        fiatOnrampStorage: FiatOnrampStoring,
        polkadotHandshakeService: PolkadotHandshakeServicing,
        signInHostCoordinator: MessageExchangeSignInHostCoordinating,
        chatExtensionsRegistry: ChatExtensionsRegistering,
        productWorkerFacade: ProductWorkerFacade,
        callCoordinator: CallCoordinating,
        chatRequestCoordinator: ChatRequestCoordinatorServicing,
        attachmentUploadService: AttachmentUploadingServicing,
        attachmentDownloadService: AttachmentDownloadingServicing,
        coinageTransferMonitor: CoinageTransferMonitoring,
        durableTransactionEngine: DurableTxServices,
        w3sPaymentTracking: W3sPaymentTracking,
        audioSessionManager: AudioSessionManaging,
        determineStateSyncService: DetermineStateSyncServicing,
        personhoodBackgroundService: PersonhoodBackgroundService,
        personDataStore: DetermineStatePersonDataStore,
        coinageBackupSyncService: CoinageBackupSyncServicing,
        messageExpansionService: CompactedMessageExpansionServicing,
        notificationBadgeSyncService: NotificationBadgeSyncService,
        accountManager: ProductsAccountManaging,
        allowanceManagerFacade: AllowanceManagerFacade,
        allowanceRenewalService: AllowanceRenewalService,
        paymentsSupport: PaymentsSupport,
        turnService: TURNCredentialsProviding,
        deviceSyncService: DeviceSyncServicing,
        networkStatusService: NetworkStatusProviding,
        chainStatusProvider: ChainStatusProviding,
        truapiRuntimeProvider: TrUAPIHostRuntimeProviding,
        tldProvider: DotNsTldProviding,
        logger: LoggerProtocol
    ) {
        self.chatCoordinator = chatCoordinator
        self.depositService = depositService
        self.fiatOnrampService = fiatOnrampService
        self.fiatOnrampTrackingService = fiatOnrampTrackingService
        self.fiatOnrampStorage = fiatOnrampStorage
        self.polkadotHandshakeService = polkadotHandshakeService
        self.signInHostCoordinator = signInHostCoordinator
        self.chatExtensionsRegistry = chatExtensionsRegistry
        self.productWorkerFacade = productWorkerFacade
        self.callCoordinator = callCoordinator
        self.chatRequestCoordinator = chatRequestCoordinator
        self.attachmentUploadService = attachmentUploadService
        self.attachmentDownloadService = attachmentDownloadService
        self.coinageTransferMonitor = coinageTransferMonitor
        self.durableTransactionEngine = durableTransactionEngine
        self.w3sPaymentTracking = w3sPaymentTracking
        self.audioSessionManager = audioSessionManager
        self.determineStateSyncService = determineStateSyncService
        self.personhoodBackgroundService = personhoodBackgroundService
        self.personDataStore = personDataStore
        self.coinageBackupSyncService = coinageBackupSyncService
        self.messageExpansionService = messageExpansionService
        self.notificationBadgeSyncService = notificationBadgeSyncService
        self.accountManager = accountManager
        self.allowanceManagerFacade = allowanceManagerFacade
        self.allowanceRenewalService = allowanceRenewalService
        self.deviceSyncService = deviceSyncService
        self.networkStatusService = networkStatusService
        self.chainStatusProvider = chainStatusProvider
        self.truapiRuntimeProvider = truapiRuntimeProvider
        self.tldProvider = tldProvider
        self.logger = logger
        self.paymentsSupport = paymentsSupport
        self.turnService = turnService
    }
}

extension ServiceCoordinator: ServiceCoordinatorProtocol {
    func setup() {
        // Keep the cached TLD warm on each launch without blocking.
        tldProvider.refresh()

        #if FEATURE_DIMS
            determineStateSyncService.setup()
            personhoodBackgroundService.setup()
        #endif

        chatCoordinator.setup()
        productWorkerFacade.setup()
        chatExtensionsRegistry.discover()
        chatRequestCoordinator.setup()
        fiatOnrampTrackingService.setup()
        attachmentUploadService.setup()
        attachmentDownloadService.setup()
        notificationBadgeSyncService.setup()
        messageExpansionService.start()
        allowanceRenewalService.setup()

        Task {
            await chainStatusProvider.start()
            await signInHostCoordinator.setup()
            await setupDeviceSyncService()

            // Setup coinage service with main asset from chain
            let chainRegistry = ChainRegistryFacade.sharedRegistry
            let mainAssetId = AppConfig.Assets.mainAsset

            guard
                let chain = chainRegistry.getChain(for: mainAssetId.chainId),
                let asset = chain.asset(for: mainAssetId.assetId)
            else {
                assertionFailure()
                return
            }
            do {
                try await coinageService.setup(with: asset)
            } catch {
                logger.error("Coinage service setup failed: \(error)")
                return
            }
            // After coinage setup, which releases uncommitted handoffs the first pass must not see.
            durableTransactionEngine.txService.start()
            // Recovering backup 1st
            await coinageTransferMonitor.setup()
            await w3sPaymentTracking.setup()
            await depositService.setup()
        }
    }

    func throttle() {
        #if FEATURE_DIMS
            determineStateSyncService.throttle()
            personhoodBackgroundService.throttle()
        #endif

        chatCoordinator.throttle()
        chatRequestCoordinator.throttle()
        fiatOnrampTrackingService.throttle()
        attachmentUploadService.throttle()
        attachmentDownloadService.throttle()
        notificationBadgeSyncService.throttle()
        allowanceRenewalService.throttle()

        messageExpansionService.stop()
        durableTransactionEngine.txService.stop()

        Task {
            await deviceSyncService.throttle()
            await coinageTransferMonitor.throttle()
            await w3sPaymentTracking.throttle()
            await signInHostCoordinator.throttle()
            await depositService.throttle()
        }
    }
}

extension ServiceCoordinator {
    // swiftlint:disable:next function_body_length
    static func createDefault(spaFlowState: SPAFlowState) -> ServiceCoordinatorProtocol? {
        let walletRepo: WalletManagerRepositoryProtocol = .shared

        // The dashboard is only reachable after onboarding cached the TLD, so built-in accounts
        // resolve synchronously here.
        guard
            let mainWallet = try? walletRepo.main(),
            let depositWallet = try? walletRepo.depositWallet()
        else {
            return nil
        }

        let logger: LoggerProtocol = Logger.shared

        guard let allowanceManagerFacade = AllowanceManagerFacade.create() else {
            return nil
        }

        let statementStoreMonitor = StatementStoreActivityMonitor()
        let chatCoordinatorFactory = MessageExchangeCoordinatorFactory(
            bulletInManager: allowanceManagerFacade.bulletInManager,
            statementStoreMonitor: statementStoreMonitor
        )

        let allowanceSupport = AllowanceSupport(
            allowancePromptRouter: ProductsRouter(),
            sssManager: allowanceManagerFacade.sssManager,
            bulletInManager: allowanceManagerFacade.bulletInManager,
            smartContractManager: allowanceManagerFacade.smartContractManager
        )

        let accountManager = ProductsAccountManager(
            entropyManager: RootEntropyManager.shared,
            allowanceSupport: allowanceSupport
        )

        let turnService = TURNCredentialsService(
            requestFactory: TURNCredentialsRequestFactory(),
            tokenProvider: JWTTokenManager.shared
        )

        let resourceKeyManager = ProductResourceKeyManager(
            keychain: Keychain(),
            userDefaults: SharedContainerGroup.userDefaults
        )

        let sponsorVrfRepo: BandersnatchManagerRepositoryProtocol = .shared
        guard let sponsorKeyResolver = try? sponsorVrfRepo.keyResolver() else {
            return nil
        }

        let sponsorFactory = HostTransactionSponsorFactory(
            accountManager: accountManager,
            resourceKeyManager: resourceKeyManager,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            keyResolver: sponsorKeyResolver,
            logger: logger
        )

        // Single process-wide TrUAPI runtime provider. Registered in the root
        // locator so the static SPA factory reaches the same instance the
        // chat bot factory does — one shared runtime across all products.
        // Host-level core confirmations route through an SSO-style facade whose
        // presentation view is attached with the main tab bar; until then,
        // host-level prompts deny.
        let truapiRuntimeProvider = TrUAPIHostRuntimeProvider(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            entropyManager: RootEntropyManager.shared,
            settingsManager: SettingsManager.shared,
            coreStorage: TrUAPILocalStorage.createCoreLocalStorage(),
            confirmationRouterFacade: ProductRoutersFacade.sso(),
            logger: logger
        )
        RootDependencyLocator.setDependency(truapiRuntimeProvider as TrUAPIHostRuntimeProviding)

        guard
            let signInHostCoordinator = createSignInHostCoordinator(
                factory: chatCoordinatorFactory,
                runtimeProvider: truapiRuntimeProvider,
                accountManager: accountManager,
                sponsorFactory: sponsorFactory,
                logger: logger
            ),
            let chatCoordinator = createChatCoordinator(factory: chatCoordinatorFactory, logger: logger),
            let coinageServices = createCoinageServices(
                allowanceManager: allowanceManagerFacade.smartContractManager
            ),
            let depositService = createDepositService(
                walletToFund: depositWallet,
                walletToDeposit: depositWallet,
                coinageService: coinageServices.coinageService
            ),
            let mainChatEncryptorFactory = try? ChatEncryptionManager().makeEncryptorFactory(
                ownEncryptionKeyId: Chat.Contact.Own.main().encryptionKeyId
            ),
            let ssoEncryptorFactory = try? ChatEncryptionManager().makeEncryptorFactory(
                ownEncryptionKeyId: Chat.Contact.Own.sso().encryptionKeyId
            ),
            let attachmentUploadService = createAttachmentUploadService(
                bulletInManager: allowanceManagerFacade.bulletInManager
            ),
            let attachmentDownloadService = createAttachmentDownloadService(),
            let syncServiceResult = createDetermineStateSyncService(),
            let deviceSyncService = try? createDeviceSyncService(turnService: turnService, logger: logger)
        else {
            return nil
        }

        guard let personhoodServices = createPersonhoodServices(
            syncStateStore: syncServiceResult.syncStore
        ) else {
            return nil
        }

        let chatRequestCoordinator = createChatRequestCoordinator()
        let audioSessionManager = AudioSessionManager()

        let paymentsSupport = PaymentsSupport(coinageService: coinageServices.coinageService)

        let truApiDependencies = TruApiDependenciesLocator()
        truApiDependencies.setDependency(allowanceSupport)
        truApiDependencies.setDependency(paymentsSupport)
        RootDependencyLocator.setDependency(truApiDependencies)

        let (chatExtensionsRegistry, productWorkerFacade) = createChatExtensionsRegistry(
            accountManager: accountManager,
            truapiRuntimeProvider: truapiRuntimeProvider,
            syncStore: syncServiceResult.syncStore,
            personDataStore: syncServiceResult.personDataStore,
            syncService: syncServiceResult.service,
            personhoodRegistrationService: personhoodServices.registrationService,
            audioSessionManager: audioSessionManager,
            spaFlowState: spaFlowState
        )
        // Registered so the SPA screen, opened outside this assembly, resolves the
        // same facade.
        RootDependencyLocator.setDependency(productWorkerFacade)

        let fiatOnrampConfiguration = MeldFiatOnrampConfiguration.prod
        let fiatOnrampStorage = FiatOnrampStorage()
        let fiatOnrampService = MeldFiatOnrampService(configuration: fiatOnrampConfiguration)
        let fiatOnrampTrackingService = FiatOnrampTrackingServicing(
            depositService: depositService,
            fiatOnrampService: fiatOnrampService,
            fiatOnrampStorage: fiatOnrampStorage,
            clock: ContinuousClock()
        )

        let callCoordinator = RealCallCoordinator(
            presentationManager: ChatChatCallPresentationManager(logger: logger),
            outboxService: chatCoordinator.outboxService,
            turnService: turnService,
            logger: logger
        )

        chatCoordinator.inboxService.setupCallCoordinator(callCoordinator)
        chatCoordinator.outboxService.setupCallCoordinator(callCoordinator)

        let notificationBadgeSyncService = NotificationBadgeSyncService(logger: logger)

        let messageExpansionService = createCompactedMessageExpansionService(logger: logger)
        let allowanceRenewalService = AllowanceRenewalService(
            managerFacade: allowanceManagerFacade,
            appStateStreamFactory: ApplicationStateStreamFactory(),
            logger: logger
        )

        let networkStatusService = NetworkStatusService(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            pathMonitor: NetworkPathMonitor()
        )

        let chainBlockProvider = ChainBlockProvider(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: logger
        )

        let chainLivenessAnchorProvider = ChainLivenessAnchorProvider(
            blockInfoProviders: createBlockInfoProviders(),
            chainTimeProviders: createChainTimeProviders()
        )

        let statementStoreStatusService = StatementStoreStatusService(
            networkStatusService: networkStatusService,
            activityMonitor: statementStoreMonitor,
            logger: logger
        )

        let chainStatusProvider = ChainStatusProvider(
            networkStatusService: networkStatusService,
            blockProvider: chainBlockProvider,
            anchorProvider: chainLivenessAnchorProvider,
            appStateStreamFactory: ApplicationStateStreamFactory(),
            statementStoreStatusProvider: statementStoreStatusService,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            logger: logger
        )

        return ServiceCoordinator(
            chatCoordinator: chatCoordinator,
            depositService: depositService,
            fiatOnrampService: fiatOnrampService,
            fiatOnrampTrackingService: fiatOnrampTrackingService,
            fiatOnrampStorage: fiatOnrampStorage,
            polkadotHandshakeService: PolkadotHandshakeService(
                rootWallet: DynamicDerivedWallet(derivationPath: nil),
                identityWallet: mainWallet,
                chatEncryptorFactory: mainChatEncryptorFactory,
                ssoEncryptorFactory: ssoEncryptorFactory,
                sssManager: allowanceManagerFacade.sssManager
            ),
            signInHostCoordinator: signInHostCoordinator,
            chatExtensionsRegistry: chatExtensionsRegistry,
            productWorkerFacade: productWorkerFacade,
            callCoordinator: callCoordinator,
            chatRequestCoordinator: chatRequestCoordinator,
            attachmentUploadService: attachmentUploadService,
            attachmentDownloadService: attachmentDownloadService,
            coinageTransferMonitor: coinageServices.transferMonitor,
            durableTransactionEngine: coinageServices.durableTransactionEngine,
            w3sPaymentTracking: coinageServices.w3sPaymentTracking,
            audioSessionManager: audioSessionManager,
            determineStateSyncService: syncServiceResult.service,
            personhoodBackgroundService: personhoodServices.backgroundService,
            personDataStore: syncServiceResult.personDataStore,
            coinageBackupSyncService: coinageServices.backupSyncService,
            messageExpansionService: messageExpansionService,
            notificationBadgeSyncService: notificationBadgeSyncService,
            accountManager: accountManager,
            allowanceManagerFacade: allowanceManagerFacade,
            allowanceRenewalService: allowanceRenewalService,
            paymentsSupport: paymentsSupport,
            turnService: turnService,
            deviceSyncService: deviceSyncService,
            networkStatusService: networkStatusService,
            chainStatusProvider: chainStatusProvider,
            truapiRuntimeProvider: truapiRuntimeProvider,
            tldProvider: DotNsTldProviderFacade.shared,
            logger: logger
        )
    }
}

private extension ServiceCoordinator {
    static func createChatCoordinator(
        factory: MessageExchangeCoordinatorMaking,
        logger: LoggerProtocol
    ) -> MessageExchangeChatCoordinating? {
        do {
            return try factory.makeChatCoordinator()
        } catch {
            logger.error("Message exchange chat coordinator error: \(error)")
            return nil
        }
    }

    /// - Note: The runtime flag is read once at coordinator creation (app start), so toggling it
    ///   takes effect on the next launch. An unset flag means TrUAPI — see `isTrUAPIRuntimeEnabled`.
    static func createSignInHostCoordinator(
        factory: MessageExchangeCoordinatorMaking,
        runtimeProvider: TrUAPIHostRuntimeProviding,
        accountManager: ProductsAccountManaging,
        sponsorFactory: TransactionSponsorMaking,
        logger: LoggerProtocol
    ) -> MessageExchangeSignInHostCoordinating? {
        do {
            if SettingsManager.shared.isTrUAPIRuntimeEnabled {
                return try factory.makeTrUAPIHostCoordinator(runtimeProvider: runtimeProvider)
            }

            return try factory.makeNativeHostCoordinator(
                accountManager: accountManager,
                sponsorFactory: sponsorFactory
            )
        } catch {
            logger.error("Message exchange sign in host coordinator error: \(error)")
            return nil
        }
    }
}

private extension ServiceCoordinator {
    static func createChatRequestCoordinator() -> ChatRequestCoordinatorServicing {
        let storageFacade = UserDataStorageFacade.shared
        let operationQueue = OperationManagerFacade.sharedDefaultQueue
        let logger = Logger.shared

        return ChatRequestCoordinatorService(
            contactsProviderFactory: ChatContactDataProviderFactory(
                repositoryFactory: ChatContactRepositoryFactory(storageFacade: storageFacade),
                operationQueue: operationQueue,
                logger: logger
            ),
            messageProviderFactory: ChatMessageDataProviderFactory(
                repositoryFactory: ChatMessageRepositoryFactory(storageFacade: storageFacade),
                operationQueue: operationQueue,
                logger: logger
            ),
            serviceFactory: ChatRequestServiceFactory(
                remoteContactResolver: CompoundRemoteContactResolver(
                    resolvers: [
                        playerContactOperation(chatChainId: AppConfig.Chains.usernameChain),
                        remoteAccountOperation(chatChainId: AppConfig.Chains.usernameChain)
                    ],
                    logger: Logger.shared
                )
            ),
            logger: Logger.shared
        )
    }

    static func playerContactOperation(chatChainId: ChainModel.Id) -> RemoteContactResolving {
        let identifierService = ChatIdentifierService(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            chain: chatChainId,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: Logger.shared
        )

        return PlayerContactOperationFactory(
            gameVotesRepositoryFactory: GameVoteRepositoryFactory(),
            identifierService: identifierService
        )
    }

    static func remoteAccountOperation(chatChainId: ChainModel.Id) -> RemoteContactOperationMaking {
        RemoteContactOperationFactory(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            connectionChainId: chatChainId,
            operationQueue: OperationManagerFacade.sharedDefaultQueue
        )
    }
}

private extension ServiceCoordinator {
    static func createDepositService(
        walletToFund: WalletManaging,
        walletToDeposit: WalletManaging,
        coinageService: CoinageServicing
    ) -> DepositServiceProtocol? {
        let logger: LoggerProtocol = Logger.shared

        guard
            let accountToFund = try? walletToFund.getMultiSigner().getAccountId() else {
            logger.error("Missing or invalid public key id")
            return nil
        }

        let stateMediatory = AssetsExchangeStateMediator()
        let priceStore = AssetExchangePriceStore(
            priceLocalSubscriptionFactory: PriceProviderFactory.shared,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            logger: logger
        )

        let assetExchangeFactory = AssetExchangeServiceFactory(
            depositWallet: walletToDeposit,
            accountToFund: accountToFund,
            fundedAssetId: AppConfig.Assets.fundedAsset,
            hydrationChainId: AppConfig.Chains.swappingChain,
            ahChainId: AppConfig.Chains.fundingChain,
            usdtChainId: AppConfig.Chains.usdtChain,
            feePercentageBuffer: DepositServiceConstants.feeBufferPercentage,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            storageFacade: SubstrateDataStorageFacade.shared,
            exchangesStateMediator: stateMediatory,
            priceStore: priceStore,
            configManager: FirebaseFacade.shared,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: logger
        )

        return try? DepositService(
            assetExchangeFactory: assetExchangeFactory,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            priceStore: priceStore,
            balanceTrackingFactory: BalanceTrackingFactory(),
            coinageService: coinageService,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: logger
        )
    }

    private static func createBlockInfoProviders() -> [ChainConnectionTarget: BlockInfoProviding] {
        ChainConnectionTarget.allCases
            .reduce(into: [ChainConnectionTarget: BlockInfoProviding]()) { accumulator, target in
                accumulator[target] = BlockInfoProvider(
                    chainRegistry: ChainRegistryFacade.sharedRegistry,
                    operationQueue: OperationManagerFacade.sharedDefaultQueue,
                    chainId: target.chainId
                )
            }
    }

    private static func createChainTimeProviders() -> [ChainConnectionTarget: ChainTimeProviding] {
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let operationQueue = OperationManagerFacade.sharedDefaultQueue

        let storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )

        var providers: [ChainConnectionTarget: ChainTimeProviding] = [:]

        for target in ChainConnectionTarget.allCases {
            providers[target] = ChainTimeProvider(
                chainId: target.chainId,
                chainRegistry: chainRegistry,
                storageRequestFactory: storageRequestFactory
            )
        }

        return providers
    }
}

// MARK: - DetermineStateSyncService Creation

private extension ServiceCoordinator {
    static func createDetermineStateSyncService() -> (
        service: DetermineStateSyncService,
        syncStore: DetermineStateSyncStore,
        personDataStore: DetermineStatePersonDataStore
    )? {
        let logger = Logger.shared
        let walletRepo: WalletManagerRepositoryProtocol = .shared
        let vrfRepo: BandersnatchManagerRepositoryProtocol = .shared

        guard
            let mainAccountId = try? walletRepo.main().getRawPublicKey(),
            let candidateAccountId = try? walletRepo.candidate().getRawPublicKey(),
            let mobRuleAccountId = try? walletRepo.mobRuleAlias().getRawPublicKey(),
            let scoreAccountId = try? walletRepo.scoreAlias().getRawPublicKey(),
            let resourcesAccountId = try? walletRepo.resourcesAlias().getRawPublicKey()
        else {
            logger.error("Failed to get wallet account IDs for DetermineStateSyncService")
            return nil
        }

        guard
            let vrfManager = try? vrfRepo.fullPerson(),
            let memberKey = try? vrfManager.getMemberKey()
        else {
            logger.error("Failed to get member key for DetermineStateSyncService")
            return nil
        }

        let syncQueue = DispatchQueue(label: "io.polkadot.app.dims.service.queue")

        let personDataStore = DetermineStatePersonDataStore(
            candidateAccountId: candidateAccountId,
            logger: logger
        )

        let syncStore = DetermineStateSyncStore(logger: logger)

        let syncService = DetermineStateSyncService(
            walletAccountId: mainAccountId,
            candidateAccountId: candidateAccountId,
            mobRuleAccountId: mobRuleAccountId,
            scoreAccountId: scoreAccountId,
            resourcesAccountId: resourcesAccountId,
            memberKey: memberKey,
            chainId: AppConfig.Chains.usernameChain,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            observers: [syncStore, personDataStore],
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            proccessingQueue: syncQueue,
            logger: logger
        )

        return (service: syncService, syncStore: syncStore, personDataStore: personDataStore)
    }
}
