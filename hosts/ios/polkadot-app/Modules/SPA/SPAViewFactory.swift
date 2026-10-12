import Foundation
import Keystore_iOS
import Operation_iOS
import Products
import KeyDerivation
import Individuality
import SubstrateSdk
import SubstrateStorageQuery
import ChainRegistry
import BulletinChain
import TrUAPIHost

enum SPAViewFactory {
    @MainActor
    static func createView(
        configuration: SPAConfiguration,
        flowState: SPAFlowState
    ) -> SPAViewProtocol? {
        let truapiEnabled = SettingsManager.shared.isTrUAPIRuntimeEnabled

        if truapiEnabled {
            return createRustView(configuration: configuration, flowState: flowState)
        }

        return createNativeView(configuration: configuration, flowState: flowState)
    }

    @MainActor
    private static func createNativeView(
        configuration: SPAConfiguration,
        flowState: SPAFlowState
    ) -> SPAViewProtocol? {
        guard let workerFacade: ProductWorkerFacade = RootDependencyLocator.getDependency() else {
            return nil
        }

        let routers = ProductRoutersFacade.spa()
        let dependencyLocator: TruApiDependenciesLocator = RootDependencyLocator.getDependency()
            ?? TruApiDependenciesLocator()
        let allowanceSupport: AllowanceSupport? = dependencyLocator.getDependency()
        let accountManager = ProductsAccountManager(
            entropyManager: RootEntropyManager.shared,
            allowanceSupport: allowanceSupport
        )

        let wireframe = SPAWireframe()

        let entropyManager = RootEntropyManager.shared

        let chainRegistry = ChainRegistryFacade.sharedRegistry

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
            chainRegistry: chainRegistry,
            keyResolver: sponsorKeyResolver,
            logger: Logger.shared
        )

        let nativeApiFactory = ProductsNativeApiFactory(
            chainRegistry: chainRegistry,
            usernameStorage: UsernameStorage(),
            localStorage: ProductsLocalStorage(
                productId: configuration.page.host.toDotDomain(),
                settingsManager: SettingsManager.shared
            ),
            notificationService: UserNotificationService.shared,
            entropyManager: entropyManager,
            dependencyLocator: dependencyLocator,
            accountManager: accountManager,
            resourceKeyManager: resourceKeyManager,
            sponsorFactory: sponsorFactory,
            substrateStorageFacade: SubstrateDataStorageFacade.shared,
            hostProvider: flowState.hostProvider,
            workerOperations: workerFacade.operations
        )

        let nativeApi = nativeApiFactory.makeApi(
            productId: configuration.page.host.toDotDomain(),
            routers: routers
        )

        let schemeHandlerProxy = SchemeHandlerProxy()

        let scriptsFactory = SPANativeRuntimeScriptsFactory(
            containerScriptProvider: BundledContainerScriptProvider()
        )

        let interactor = SPANativeRuntimeInteractor(
            nativeApi: nativeApi,
            scriptsFactory: scriptsFactory,
            dotNsResolver: flowState.dotNsResolver,
            productResolver: flowState.productResolver,
            schemeHandlerProxy: schemeHandlerProxy,
            configuration: configuration,
            logger: Logger.shared,
            productRepository: ProductRepositoryFactory().createRepository(),
            chatProviderFactory: ChatContactDataProviderFactory(),
            workerManager: workerFacade.manager,
            workerProductId: configuration.page.host.toDotDomain()
        )

        let presenter = SPAPresenter(
            interactor: interactor,
            wireframe: wireframe,
            configuration: configuration,
            hostProvider: flowState.hostProvider
        )
        let view = createViewController(
            presenter: presenter,
            configuration: configuration,
            schemeHandlerProxy: schemeHandlerProxy,
            hostProvider: flowState.hostProvider
        )

        presenter.view = view
        interactor.presenter = presenter

        routers.setPresentationView(view)

        if !configuration.isRootScreen {
            view.controller.hidesBottomBarWhenPushed = true
        }

        return view
    }

    @MainActor
    static func createView(
        page: ProductPage,
        flowState: SPAFlowState,
        isBrowserTab: Bool = false,
        browserTabId: UUID? = nil
    ) -> SPAViewProtocol? {
        let configuration = SPAConfiguration(
            title: nil,
            isRootScreen: false,
            showMoreButton: true,
            page: page,
            isBrowserTab: isBrowserTab,
            browserTabId: browserTabId
        )

        return createView(
            configuration: configuration,
            flowState: flowState
        )
    }
}

// MARK: - Rust runtime assembly

extension SPAViewFactory {
    /// Internal so the debug playground launcher can assemble a rust SPA view.
    /// The shared ``TrUAPIHostRuntime`` is sourced from the process-wide
    /// provider registered by `ServiceCoordinator`.
    @MainActor
    static func createRustView(
        configuration: SPAConfiguration,
        flowState: SPAFlowState
    ) -> SPAViewProtocol? {
        let routers = ProductRoutersFacade.spa()
        let wireframe = SPAWireframe()
        let schemeHandlerProxy = SchemeHandlerProxy()

        guard let runtimeProvider: TrUAPIHostRuntimeProviding = RootDependencyLocator.getDependency() else {
            Logger.shared.error("Rust SPA unavailable: TrUAPI runtime provider missing")
            return nil
        }

        let gameReminders: ProductGameReminderScheduling? = RootDependencyLocator.getDependency()

        let runtime: TrUAPIHostRuntime
        do {
            runtime = try runtimeProvider.sharedRuntime()
        } catch {
            Logger.shared.error("Rust SPA unavailable: \(error)")
            return nil
        }

        let rustEnvironment = RustRuntimeEnvironment(
            runtime: runtime,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            notificationScheduler: ProductNotificationScheduler.shared,
            gameReminders: gameReminders,
            ipfsFetcher: IpfsFetcher(ipfsBaseURL: AppConfig.KnownIPFS.main),
            hostProvider: flowState.hostProvider,
            logger: Logger.shared
        )

        let runtimeFactory = SPARustRuntimeFactory(environment: .init(
            rust: rustEnvironment,
            configuration: configuration,
            dotNsResolver: flowState.dotNsResolver,
            productResolver: flowState.productResolver,
            schemeHandlerProxy: schemeHandlerProxy,
            routers: routers
        ))

        let interactor = SPARustRuntimeInteractor(
            runtimeFactory: runtimeFactory,
            productResolver: flowState.productResolver,
            configuration: configuration,
            logger: Logger.shared,
            productRepository: ProductRepositoryFactory().createRepository(),
            chatProviderFactory: ChatContactDataProviderFactory()
        )

        let presenter = SPAPresenter(
            interactor: interactor,
            wireframe: wireframe,
            configuration: configuration,
            hostProvider: flowState.hostProvider
        )

        let view = createViewController(
            presenter: presenter,
            configuration: configuration,
            schemeHandlerProxy: schemeHandlerProxy,
            hostProvider: flowState.hostProvider
        )

        presenter.view = view
        interactor.presenter = presenter
        runtimeFactory.setPresentationView(view)

        if !configuration.isRootScreen {
            view.controller.hidesBottomBarWhenPushed = true
        }

        return view
    }
}

// MARK: - Private

private extension SPAViewFactory {
    /// The browser chrome — floating pills, chrome collapse, minimize — only makes sense where
    /// products live on as tabs, so builds without the browse tab get the simplified screen.
    @MainActor
    static func createViewController(
        presenter: SPAPresenterProtocol,
        configuration: SPAConfiguration,
        schemeHandlerProxy: SchemeHandlerProxy,
        hostProvider: ProductHostProviding
    ) -> SPAViewProtocol {
        #if FEATURE_PRODUCTS
            return SPAViewController(
                presenter: presenter,
                configuration: configuration,
                schemeHandlerProxy: schemeHandlerProxy,
                logger: Logger.shared,
                hostProvider: hostProvider
            )
        #else
            return SPASimplifiedViewController(
                presenter: presenter,
                configuration: configuration,
                schemeHandlerProxy: schemeHandlerProxy,
                logger: Logger.shared,
                hostProvider: hostProvider
            )
        #endif
    }
}
