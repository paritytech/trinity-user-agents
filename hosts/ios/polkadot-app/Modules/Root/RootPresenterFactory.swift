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
        let flowStateProvider = DashboardServices.flowStateProvider
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
            initializer: AppSharedInitializer.shared,
            logger: Logger.shared,
            resolver: resolver,
            tokenManager: JWTTokenManager.shared,
            remoteConfigManager: FirebaseFacade.shared,
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
                identityChain: AppConfig.Chains.assethubChain
            )
        #endif

        let initViewController = RootViewController(presenter: presenter)
        presenter.view = initViewController
        window.rootViewController = initViewController

        return presenter
    }

    @MainActor
    private static func createProductLabels(flowStateProvider: SPAFlowStateProviding) async -> [String] {
        let staticProducts = [AppConfig.DotNs.dotNsBrowse]

        let fundingProvider = FundingDomainProvider(
            hostProvider: flowStateProvider.flowState().hostProvider
        )

        let fundingPages = await [
            try? fundingProvider.fundingPage(),
            try? fundingProvider.offrampPage()
        ]

        return staticProducts + fundingPages.compactMap { $0?.host.name }
    }
}
