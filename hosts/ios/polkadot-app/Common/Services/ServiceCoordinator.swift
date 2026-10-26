import Foundation
import BackgroundExecution
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
import StructuredConcurrency
import DesignSystem

protocol ServiceCoordinatorProtocol: DashboardServicesCoordinating {
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
    var coinageService: CoinageServicing { get }
    var coinageBackupSyncService: CoinageBackupSyncServicing { get }
    var accountManager: ProductsAccountManaging { get }
    var allowanceManagerFacade: AllowanceManagerFacade { get }
    var turnService: TURNCredentialsProviding { get }
    var networkStatusService: NetworkStatusProviding { get }
    var chainStatusProvider: ChainStatusProviding { get }
    var truapiRuntimeProvider: TrUAPIHostRuntimeProviding { get }
    var durabilityRenewal: StatementDurabilityRenewing { get }
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
    let coinageBackupSyncService: CoinageBackupSyncServicing
    let messageExpansionService: CompactedMessageExpansionServicing
    let notificationBadgeSyncService: NotificationBadgeSyncService
    let accountManager: ProductsAccountManaging
    let allowanceManagerFacade: AllowanceManagerFacade
    let statementRenewalLoop: StatementRenewalLoop
    let durabilityRenewal: StatementDurabilityRenewing
    let turnService: TURNCredentialsProviding
    let deviceSyncService: DeviceSyncServicing
    let networkStatusService: NetworkStatusProviding
    let chainStatusProvider: ChainStatusProviding
    let truapiRuntimeProvider: TrUAPIHostRuntimeProviding
    let tldProvider: DotNsTldProviding
    /// Held rather than looked up, so the session that started the Pocket is
    /// the one that stops it. Nil in a build without the products feature.
    let pocket: ProductPocketService?
    let logger: LoggerProtocol

    // Retained so the weakly-held dependency-locator entry stays alive for the product host.
    let paymentsSupport: PaymentsSupport

    // Lifecycle state, touched only from the main actor by DashboardServicesManager.
    var durabilityStart: Task<Bool, Never>?
    var interactiveStart: Task<Void, Never>?

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
        coinageBackupSyncService: CoinageBackupSyncServicing,
        messageExpansionService: CompactedMessageExpansionServicing,
        notificationBadgeSyncService: NotificationBadgeSyncService,
        accountManager: ProductsAccountManaging,
        allowanceManagerFacade: AllowanceManagerFacade,
        statementRenewalLoop: StatementRenewalLoop,
        durabilityRenewal: StatementDurabilityRenewing,
        paymentsSupport: PaymentsSupport,
        turnService: TURNCredentialsProviding,
        deviceSyncService: DeviceSyncServicing,
        networkStatusService: NetworkStatusProviding,
        chainStatusProvider: ChainStatusProviding,
        truapiRuntimeProvider: TrUAPIHostRuntimeProviding,
        tldProvider: DotNsTldProviding,
        pocket: ProductPocketService?,
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
        self.coinageBackupSyncService = coinageBackupSyncService
        self.messageExpansionService = messageExpansionService
        self.notificationBadgeSyncService = notificationBadgeSyncService
        self.accountManager = accountManager
        self.allowanceManagerFacade = allowanceManagerFacade
        self.statementRenewalLoop = statementRenewalLoop
        self.durabilityRenewal = durabilityRenewal
        self.deviceSyncService = deviceSyncService
        self.networkStatusService = networkStatusService
        self.chainStatusProvider = chainStatusProvider
        self.truapiRuntimeProvider = truapiRuntimeProvider
        self.tldProvider = tldProvider
        self.pocket = pocket
        self.logger = logger
        self.paymentsSupport = paymentsSupport
        self.turnService = turnService
    }
}

extension ServiceCoordinator: ServiceCoordinatorProtocol {}

extension ServiceCoordinator {
    @MainActor
    // swiftlint:disable:next function_body_length
    static func createDefault(spaFlowState: SPAFlowState) -> ServiceCoordinator? {
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
        let identitySigningFallback = NonPrivateSigningFallback.makeDefault()
        let chatCoordinatorFactory = MessageExchangeCoordinatorFactory(
            bulletInManager: allowanceManagerFacade.bulletInManager,
            statementAccountFunding: allowanceManagerFacade.sssManager,
            identitySigningFallback: identitySigningFallback,
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
        guard let personKeyResolver = try? sponsorVrfRepo.keyResolver() else {
            return nil
        }

        let sponsorFactory = HostTransactionSponsorFactory(
            accountManager: accountManager,
            resourceKeyManager: resourceKeyManager,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            keyResolver: personKeyResolver,
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
        CoreDurableRecoveryTask.shared.attach(
            truapiRuntimeProvider,
            executor: ConnectionRetainingExecutor(provider: ChainRegistryFacade.sharedRegistry)
        )

        let gameReminders = ProductGameReminderCenter.makeDefault()
        RootDependencyLocator.setDependency(gameReminders as ProductGameReminderScheduling)
        RootDependencyLocator.setDependency(gameReminders as ProductGamePillProviding)

        let productFileProvider = CompositeProductFileProvider(
            dotNsContentStorage: DotNsContentStorage(),
            chatScriptStorage: FileChatScriptStorage(),
            contentHashCache: ContentHashCache.shared
        )

        // Built here rather than alongside chat: every modality is served from
        // one worker per product, and hanging the Pocket off chat's assembly
        // would let any chat service failing take every live card face with it,
        // silently.
        var pocket: ProductPocketService?
        if let service = ProductPocketService.make() {
            RootDependencyLocator.setDependency(service)
            service.start(
                runtimeProvider: truapiRuntimeProvider,
                flowState: spaFlowState,
                productFileProvider: productFileProvider,
                chainRegistry: ChainRegistryFacade.sharedRegistry,
                gameReminders: gameReminders,
                themeManager: MainActor.assumeIsolated { ThemeManager.shared }
            )
            pocket = service
        }

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
            let deviceSyncService = try? createDeviceSyncService(turnService: turnService, logger: logger),
            let notificationAllocator = try? createNotificationAllocator(
                durable: coinageServices.durableTransactionEngine,
                keyResolver: personKeyResolver
            )
        else {
            return nil
        }

        let chatRequestServiceFactory = createChatRequestServiceFactory(
            notificationAllocator: notificationAllocator,
            signingFallback: identitySigningFallback
        )
        let durabilityRenewal = StatementDurabilityRenewalService(
            allowances: allowanceManagerFacade,
            renewers: chatRequestServiceFactory,
            issues: StatementRenewalIssueDiagnostics.make(reporter: IssueReportingFacade.shared),
            sharedRun: SharedRun(cancelledValue: .cancelled),
            logger: logger
        )
        let chatRequestCoordinator = createChatRequestCoordinator(serviceFactory: chatRequestServiceFactory)
        let audioSessionManager = AudioSessionManager()

        let paymentsSupport = PaymentsSupport(coinageService: coinageServices.coinageService)
        truapiRuntimeProvider.attach(paymentsSupport: paymentsSupport, hostProvider: spaFlowState.hostProvider)

        let truApiDependencies = TruApiDependenciesLocator()
        truApiDependencies.setDependency(allowanceSupport)
        truApiDependencies.setDependency(paymentsSupport)
        RootDependencyLocator.setDependency(truApiDependencies)

        let (chatExtensionsRegistry, productWorkerFacade) = createChatExtensionsRegistry(
            accountManager: accountManager,
            truapiRuntimeProvider: truapiRuntimeProvider,
            spaFlowState: spaFlowState,
            productFileProvider: productFileProvider,
            pocket: pocket
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
        let statementRenewalLoop = StatementRenewalLoop(
            durabilityRenewal: durabilityRenewal,
            backgroundExecutor: ConnectionRetainingExecutor(provider: ChainRegistryFacade.sharedRegistry)
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
            coinageBackupSyncService: coinageServices.backupSyncService,
            messageExpansionService: messageExpansionService,
            notificationBadgeSyncService: notificationBadgeSyncService,
            accountManager: accountManager,
            allowanceManagerFacade: allowanceManagerFacade,
            statementRenewalLoop: statementRenewalLoop,
            durabilityRenewal: durabilityRenewal,
            paymentsSupport: paymentsSupport,
            turnService: turnService,
            deviceSyncService: deviceSyncService,
            networkStatusService: networkStatusService,
            chainStatusProvider: chainStatusProvider,
            truapiRuntimeProvider: truapiRuntimeProvider,
            tldProvider: DotNsTldProviderFacade.shared,
            pocket: pocket,
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
    static func createChatRequestServiceFactory(
        notificationAllocator: NotificationStatementAccountAllocating,
        signingFallback: NonPrivateSigningFallbacking
    ) -> ChatRequestServiceFactory {
        ChatRequestServiceFactory(
            remoteContactResolver: CompoundRemoteContactResolver(
                resolvers: [
                    remoteAccountOperation(chatChainId: AppConfig.Chains.assethubChain)
                ],
                logger: Logger.shared
            ),
            notificationAllocator: notificationAllocator,
            signingFallback: signingFallback,
            backgroundExecutor: ConnectionRetainingExecutor(provider: ChainRegistryFacade.sharedRegistry),
            issueReporter: IssueReportingFacade.shared
        )
    }

    static func createChatRequestCoordinator(
        serviceFactory: ChatRequestServiceFactory
    ) -> ChatRequestCoordinatorServicing {
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
            serviceFactory: serviceFactory,
            logger: logger
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
