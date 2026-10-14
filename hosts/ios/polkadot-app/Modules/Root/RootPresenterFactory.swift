import UIKit
import Keystore_iOS
import Operation_iOS
import KeyDerivation
import SubstrateSdk
import ChainRegistry

enum RootPresenterFactory: RootPresenterFactoryProtocol {
    static func createPresenter(
        with window: UIWindow
    ) -> RootPresenterProtocol {
        let flowStateProvider = SPAFlowStateProvider()
        let foregroundPresentationController = PushForegroundPresentationController()

        let chatRouteHandler = PeerChatPushRouteHandler(
            moduleNavigator: ModuleNavigator(),
            visibilityReporter: foregroundPresentationController
        )

        let chatExtensionRouters: [ChatExtensionPushRouting] = []

        let chatExtensionRouteHandler = ChatExtensionPushRouteHandler(
            routers: chatExtensionRouters,
            moduleNavigator: ModuleNavigator(),
            visibilityReporter: foregroundPresentationController
        )

        let deeplinkRouteHandler = DeeplinkPushRouteHandler()

        let pushHandler = PushHandler(
            routeBuilder: PushRouteBuilder(),
            handlers: [chatRouteHandler, chatExtensionRouteHandler, deeplinkRouteHandler]
        )

        let userNotificationService = UserNotificationService.shared
        userNotificationService.setupHandlers(
            pushTapHandler: pushHandler,
            foregroundPresentationDecider: foregroundPresentationController
        )
        let wireframe = RootWireframe(
            window: window,
            userNotificationService: userNotificationService,
            foregroundVisibilityReporter: foregroundPresentationController,
            deepLinkHandling: DeferredLinkHandler.shared,
            flowStateProvider: flowStateProvider
        )

        let migrator = createLaunchMigrator()

        let resolver = SequentialDecisionResolver<RootDestination>(
            gates: [
                RootGate.Theme(),
                RootGate.Wallet(
                    entropyManager: RootEntropyManager.shared,
                    backupHelper: MnemonicBackupHelper()
                ),
                RootGate.Username(usernameStorage: UsernameStorage())
            ],
            fallback: .dashboard
        )

        let chainRegistryClosure = { ChainRegistryFacade.sharedRegistry }

        let productPrewarmer = ProductContentPrewarmer(
            makeLabels: { await createProductLabels(flowStateProvider: flowStateProvider) },
            chainRegistryClosure: chainRegistryClosure,
            flowStateProvider: flowStateProvider
        )

        let interactor = RootInteractor(
            chainRegistryClosure: chainRegistryClosure,
            migrator: migrator,
            logger: Logger.shared,
            resolver: resolver,
            tokenManager: JWTTokenManager.shared,
            remoteConfigManager: FirebaseFacade.shared,
            chainRegistryConfigurator: FirebaseFacade.shared,
            productPrewarmer: productPrewarmer,
            observer: RootSetupObserver(pathMonitor: NetworkPathMonitor())
        )

        let presenter = RootPresenter(
            wireframe: wireframe,
            interactor: interactor,
            viewModelFactory: RootViewModelFactory()
        )

        interactor.presenter = presenter

        #if TESTNET_FEATURE
            interactor.appFactoryResetCheckerFactory = AppFactoryResetCheckerFactory(
                operationQueue: OperationManagerFacade.sharedDefaultQueue,
                usernameChain: AppConfig.Chains.usernameChain
            )
        #endif

        let initViewController = RootViewController(presenter: presenter)
        presenter.view = initViewController
        window.rootViewController = initViewController

        return presenter
    }

    @MainActor
    private static func createProductLabels(flowStateProvider: SPAFlowStateProviding) async -> [String] {
        #if FEATURE_PRODUCTS
            let staticProducts = [AppConfig.DotNs.dotNsBrowse]
        #else
            let staticProducts: [String] = []
        #endif

        let fundingProvider = FundingDomainProvider(
            hostProvider: flowStateProvider.flowState().hostProvider
        )

        let fundingPages = await [
            try? fundingProvider.fundingPage(),
            try? fundingProvider.offrampPage()
        ]

        return staticProducts + fundingPages.compactMap { $0?.host.name }
    }

    /// Local launch steps in order: erase a cross-device backup restore before any store is opened, then
    /// migrate the schemas.
    private static func createLaunchMigrator() -> Migrating {
        let restoredBackupGuard = RestoredBackupGuard(
            keyIdStore: InstallationKeyIdStore(),
            entropyManager: RootEntropyManager.shared,
            eraser: LocalStateEraser(logger: Logger.shared),
            logger: Logger.shared
        )

        let userStorageMigrator = UserStorageMigrator(
            storeURL: UserStorageParams.storageURL,
            modelDirectory: UserStorageParams.modelDirectory,
            model: UserStorageParams.modelVersion,
            fileManager: FileManager.default
        )

        let substrateStorageMigrator = SubstrateStorageMigrator(
            storeURL: SubstrateStorageParams.storageURL,
            modelDirectory: SubstrateStorageParams.modelDirectory,
            model: SubstrateStorageParams.modelVersion,
            fileManager: FileManager.default
        )

        return SerialMigrator(migrations: [restoredBackupGuard, userStorageMigrator, substrateStorageMigrator])
    }
}
