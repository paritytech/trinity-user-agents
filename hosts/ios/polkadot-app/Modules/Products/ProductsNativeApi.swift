import Foundation
import os
import Coinage
import Products
import SubstrateSdk
import KeyDerivation
import DesignSystem
import ChainRegistry

/// Concrete implementation of ``ProductsNativeApiProtocol`` that bridges
/// JS bot commands to native wallet, chain registry, and chat capabilities.
final class ProductsNativeApi: ProductsNativeApiProtocol, @unchecked Sendable {
    /// Chat messaging is bound while a chat surface drives this product's worker
    /// and cleared when it detaches, so a single shared worker can serve chat
    /// only while chat is open. Non-chat consumers (SPA, operations) never bind.
    private let messaging = OSAllocatedUnfairLock<MessagingSupport?>(initialState: nil)
    let chainRegistry: ChainRegistryProtocol
    let usernameStorage: UsernameStoring
    let productsRouter: ProductsRouting
    let navigationRouter: ProductsNavigationRouting
    let localStorage: ProductLocalStorageProtocol
    let notificationService: UserNotificationServicing
    let notificationScheduler: ProductNotificationScheduling
    let entropyManager: RootEntropyManaging
    let entropyDeriver: ProductRootEntropyDeriving
    let substrateStorageFacade: StorageFacadeProtocol
    let permissionGuard: ProductPermissionGuarding
    let paymentApprovalRequester: PaymentApprovalRequesting
    let paymentPrivacyConfirmer: PaymentPrivacyConfirming
    let recyclingStrategy: any CoinageRecyclingStrategyProviding
    let paymentsSupport: PaymentsSupport?
    let accountManager: ProductsAccountManaging
    let createProofHandler: APCreateProofHandling
    let aliasHandler: APAliasHandling
    let signVrfHandler: APSignVrfHandling
    let resourceKeyManager: ProductResourceKeyManaging
    let sponsorFactory: TransactionSponsorMaking
    let themeManager: ThemeManagerProtocol
    let localeProvider: LocaleProviding
    let productId: ProductId
    let hostProvider: ProductHostProviding
    let workerOperations: ProductWorkerOperating

    lazy var preimageSponsor: PreimageSubmitSponsoring = sponsorFactory.makePreimageSponsor()
    lazy var statementStoreSponsor: StatementStoreSponsoring = sponsorFactory.makeStatementStoreSponsor()
    lazy var signingHandler: TransactionSigningHandling = TransactionSigningHandler(
        pgasSponsor: sponsorFactory.makePGasSponsor(),
        chainRegistry: chainRegistry,
        router: productsRouter,
        logger: logger
    )

    lazy var accountResolver: IdentityAccountResolving = IdentityAccountResolver()

    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    init(
        productId: ProductId,
        messagingSupport: MessagingSupport?,
        chainRegistry: ChainRegistryProtocol,
        usernameStorage: UsernameStoring,
        productsRouter: ProductsRouting,
        navigationRouter: ProductsNavigationRouting,
        localStorage: ProductLocalStorageProtocol,
        notificationService: UserNotificationServicing,
        notificationScheduler: ProductNotificationScheduling,
        entropyManager: RootEntropyManaging,
        entropyDeriver: ProductRootEntropyDeriving,
        substrateStorageFacade: StorageFacadeProtocol,
        permissionGuard: ProductPermissionGuarding,
        paymentApprovalRequester: PaymentApprovalRequesting,
        paymentPrivacyConfirmer: PaymentPrivacyConfirming,
        recyclingStrategy: any CoinageRecyclingStrategyProviding,
        paymentsSupport: PaymentsSupport?,
        accountManager: ProductsAccountManaging,
        createProofHandler: APCreateProofHandling,
        aliasHandler: APAliasHandling,
        signVrfHandler: APSignVrfHandling,
        resourceKeyManager: ProductResourceKeyManaging,
        sponsorFactory: TransactionSponsorMaking,
        themeManager: ThemeManagerProtocol,
        localeProvider: LocaleProviding,
        hostProvider: ProductHostProviding,
        workerOperations: ProductWorkerOperating,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        messaging.withLock { $0 = messagingSupport }
        self.productId = productId
        self.chainRegistry = chainRegistry
        self.usernameStorage = usernameStorage
        self.productsRouter = productsRouter
        self.navigationRouter = navigationRouter
        self.localStorage = localStorage
        self.notificationService = notificationService
        self.notificationScheduler = notificationScheduler
        self.entropyManager = entropyManager
        self.entropyDeriver = entropyDeriver
        self.substrateStorageFacade = substrateStorageFacade
        self.permissionGuard = permissionGuard
        self.paymentApprovalRequester = paymentApprovalRequester
        self.paymentPrivacyConfirmer = paymentPrivacyConfirmer
        self.recyclingStrategy = recyclingStrategy
        self.paymentsSupport = paymentsSupport
        self.accountManager = accountManager
        self.createProofHandler = createProofHandler
        self.aliasHandler = aliasHandler
        self.signVrfHandler = signVrfHandler
        self.resourceKeyManager = resourceKeyManager
        self.sponsorFactory = sponsorFactory
        self.themeManager = themeManager
        self.localeProvider = localeProvider
        self.hostProvider = hostProvider
        self.workerOperations = workerOperations
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension ProductsNativeApi {
    struct MessagingSupport {
        weak var bot: (any ChatExtensionBotProtocol)?
        let context: ChatExtensionDiscoverContextProtocol?
    }

    /// Bind the active chat surface so outgoing messages route to it. Called by
    /// the chat runtime when it attaches to this product's shared worker.
    func bindMessaging(_ support: MessagingSupport) {
        messaging.withLock { $0 = support }
    }

    /// Clear the chat binding when the chat surface detaches. Outgoing message
    /// calls then fail with ``ProductNativeApiError/messagesNotSupported``.
    func unbindMessaging() {
        messaging.withLock { $0 = nil }
    }

    /// A single consistent read of the current chat binding, so a rebind cannot
    /// tear a `bot`/`context` pair across two separate reads.
    var currentMessaging: MessagingSupport? {
        messaging.withLock { $0 }
    }
}
