import Foundation
import Coinage
import Products
import StatementStore
import KeyDerivation
import DesignSystem
import ChainRegistry

protocol ProductsNativeApiMaking {
    func makeApi(
        messagingSupport: ProductsNativeApi.MessagingSupport?,
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol
    ) -> any ProductsNativeApiProtocol
}

extension ProductsNativeApiMaking {
    func makeApi(
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol
    ) -> any ProductsNativeApiProtocol {
        makeApi(
            messagingSupport: nil,
            productId: productId,
            routers: routers
        )
    }
}

final class ProductsNativeApiFactory: ProductsNativeApiMaking {
    private let chainRegistry: ChainRegistryProtocol
    private let usernameStorage: UsernameStoring
    private let localStorage: ProductLocalStorageProtocol
    private let notificationService: UserNotificationServicing
    private let notificationScheduler: ProductNotificationScheduling
    private let entropyManager: RootEntropyManaging
    private let substrateStorageFacade: StorageFacadeProtocol
    private let permissionRepository: ProductPermissionRepositoryProtocol
    private let osPermissionAsker: OSPermissionAsking
    private let paymentsSupport: PaymentsSupport?
    private let accountManager: ProductsAccountManaging
    private let resourceKeyManager: ProductResourceKeyManaging
    private let sponsorFactory: TransactionSponsorMaking
    private let themeManager: ThemeManagerProtocol
    private let localeProvider: LocaleProviding
    private let hostProvider: ProductHostProviding
    private let workerOperations: ProductWorkerOperating
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        chainRegistry: ChainRegistryProtocol,
        usernameStorage: UsernameStoring,
        localStorage: ProductLocalStorageProtocol,
        notificationService: UserNotificationServicing,
        notificationScheduler: ProductNotificationScheduling = ProductNotificationScheduler.shared,
        entropyManager: RootEntropyManaging,
        dependencyLocator: any DependencyLocator,
        accountManager: ProductsAccountManaging,
        resourceKeyManager: ProductResourceKeyManaging,
        sponsorFactory: TransactionSponsorMaking,
        substrateStorageFacade: StorageFacadeProtocol = SubstrateDataStorageFacade.shared,
        permissionRepository: ProductPermissionRepositoryProtocol = ProductPermissionRepository(),
        osPermissionAsker: OSPermissionAsking = OSPermissionAsker(),
        themeManager: ThemeManagerProtocol = ThemeManager.shared,
        localeProvider: LocaleProviding = SystemLocaleProvider.shared,
        hostProvider: ProductHostProviding,
        workerOperations: ProductWorkerOperating,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chainRegistry = chainRegistry
        self.usernameStorage = usernameStorage
        self.localStorage = localStorage
        self.notificationService = notificationService
        self.notificationScheduler = notificationScheduler
        self.entropyManager = entropyManager
        self.accountManager = accountManager
        self.resourceKeyManager = resourceKeyManager
        self.sponsorFactory = sponsorFactory
        self.substrateStorageFacade = substrateStorageFacade
        self.permissionRepository = permissionRepository
        self.osPermissionAsker = osPermissionAsker
        self.themeManager = themeManager
        self.localeProvider = localeProvider
        self.hostProvider = hostProvider
        self.workerOperations = workerOperations
        self.operationQueue = operationQueue
        self.logger = logger

        paymentsSupport = dependencyLocator.getDependency()
    }

    func makeApi(
        messagingSupport: ProductsNativeApi.MessagingSupport?,
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol
    ) -> any ProductsNativeApiProtocol {
        let permissionGuard = ProductPermissionGuard.create(
            router: routers.productsRouter,
            repository: permissionRepository,
            osAsker: osPermissionAsker
        )
        let entropyDeriver = ProductRootEntropyDeriver(entropyManager: entropyManager)
        let personhoodHandlerFactory = makePersonhoodHandlerFactory(routers: routers)

        return ProductsNativeApi(
            productId: productId,
            messagingSupport: messagingSupport,
            chainRegistry: chainRegistry,
            usernameStorage: usernameStorage,
            productsRouter: routers.productsRouter,
            navigationRouter: routers.navigationRouter,
            localStorage: localStorage,
            notificationService: notificationService,
            notificationScheduler: notificationScheduler,
            entropyManager: entropyManager,
            entropyDeriver: entropyDeriver,
            substrateStorageFacade: substrateStorageFacade,
            permissionGuard: permissionGuard,
            paymentApprovalRequester: PaymentApprovalRequesterFactory.create(router: routers.productsRouter),
            paymentPrivacyConfirmer: PaymentPrivacyConfirmer(router: routers.productsRouter),
            recyclingStrategy: CoinageRecyclingStrategyStore.shared,
            paymentsSupport: paymentsSupport,
            accountManager: accountManager,
            createProofHandler: personhoodHandlerFactory.makeCreateProofHandler(callingProductId: productId),
            aliasHandler: personhoodHandlerFactory.makeAliasHandler(callingProductId: productId),
            signVrfHandler: personhoodHandlerFactory.makeSignVrfHandler(callingProductId: productId),
            resourceKeyManager: resourceKeyManager,
            sponsorFactory: sponsorFactory,
            themeManager: themeManager,
            localeProvider: localeProvider,
            hostProvider: hostProvider,
            workerOperations: workerOperations,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}

// MARK: - Private

private extension ProductsNativeApiFactory {
    func makePersonhoodHandlerFactory(routers: ProductRoutersFacadeProtocol) -> APPersonhoodHandlerFactory {
        APPersonhoodHandlerFactory(
            chainRegistry: chainRegistry,
            routers: routers,
            entropyManager: entropyManager,
            permissionRepository: permissionRepository,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}
