import UIKit
import Keystore_iOS
import Operation_iOS
import UIKitExt
import Products

enum MainTabBarViewFactory {
    @MainActor
    static func createView(
        userNotificationService: UserNotificationServicing,
        foregroundVisibilityReporter: PushForegroundVisibilityReporting? = nil,
        deepLinkHandling: DeferredLinkHandling,
        flowStateProvider: any SPAFlowStateProviding
    ) -> MainTabBarViewProtocol? {
        let spaFlowState = flowStateProvider.flowState()

        guard let serviceCoordinator = ServiceCoordinator.createDefault(spaFlowState: spaFlowState) else {
            return nil
        }

        let polkadotSignInService = PolkadotSignInService(
            polkadotHandshakeService: serviceCoordinator.polkadotHandshakeService
        )

        let storageFacade = UserDataStorageFacade.shared
        let flowState = createFlowState(
            serviceCoordinator: serviceCoordinator,
            storageFacade: storageFacade,
            userNotificationService: userNotificationService,
            foregroundVisibilityReporter: foregroundVisibilityReporter,
            spaFlowState: spaFlowState
        )

        let moduleNavigator = ModuleNavigator()
        let urlHandler = createURLHandler(
            polkadotSignInService: polkadotSignInService,
            storageFacade: storageFacade,
            serviceCoordinator: serviceCoordinator,
            flowState: flowState,
            moduleNavigator: moduleNavigator,
            hostProvider: spaFlowState.hostProvider
        )

        let mnemonicBackupHelper = MnemonicBackupHelper()
        let browserCoordinator = createBrowserCoordinator(flowStateProvider: flowStateProvider)
        let interactor = MainTabBarInteractor(
            serviceCoordinator: serviceCoordinator,
            chainStatusProvider: serviceCoordinator.chainStatusProvider,
            userNotificationService: userNotificationService,
            urlHandlingService: urlHandler,
            deferredLinkHandler: deepLinkHandling,
            mnemonicBackupHelper: mnemonicBackupHelper,
            browserCoordinator: browserCoordinator,
            productGamePills: RootDependencyLocator.getDependency()
        )

        let qrHandler = WalletQRScanResultHandler(
            dsfinvkRouter: W3sDsfinvkRouter.createDefault(coinageService: serviceCoordinator.coinageService)
        )

        let wireframe = MainTabBarWireframe(
            serviceCoordinator: serviceCoordinator,
            scanResultHandler: qrHandler,
            moduleNavigator: moduleNavigator
        )

        let chipViewModelFactory = SPATabChipViewModelFactory(
            flowStateProvider: flowStateProvider
        )
        let tabFactory = TabFactory(
            serviceCoordinator: serviceCoordinator,
            flowState: flowState,
            scanResultHandler: qrHandler,
            flowStateProvider: flowStateProvider,
            foregroundVisibilityReporter: foregroundVisibilityReporter
        )

        let presenter = MainTabBarPresenter(
            interactor: interactor,
            wireframe: wireframe,
            chipViewModelFactory: chipViewModelFactory
        )

        interactor.presenter = presenter
        polkadotSignInService.output = interactor

        let view = MainTabBarViewController(
            presenter: presenter,
            viewFactory: tabFactory,
            browserCoordinator: browserCoordinator,
            flowStateProvider: flowStateProvider
        )

        presenter.view = view
        presenter.configureViews()

        configurePresentationView(view: view, in: serviceCoordinator)

        return view
    }

    private static func createFlowState(
        serviceCoordinator: ServiceCoordinatorProtocol,
        storageFacade: StorageFacadeProtocol,
        userNotificationService: UserNotificationServicing,
        foregroundVisibilityReporter: PushForegroundVisibilityReporting?,
        spaFlowState: SPAFlowState
    ) -> ChatFlowState {
        let contactsRepository = storageFacade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ChatContactMapper())
        )

        let messageDecoder = ChatPushMessageCoder(encryptionManager: ChatEncryptionManager())
        let notificationsCleaner = PushNotificationsCleaner(
            notificationService: userNotificationService,
            contactRepository: AnyDataProviderRepository(contactsRepository),
            messageDecoder: messageDecoder
        )

        return ChatFlowState(
            extensionsRegistry: serviceCoordinator.chatExtensionsRegistry,
            callCoordinator: serviceCoordinator.callCoordinator,
            outboxService: serviceCoordinator.chatCoordinator.outboxService,
            attachmentUploadStateProvider: serviceCoordinator.attachmentUploadService,
            attachmentDownloadStateProvider: serviceCoordinator.attachmentDownloadService,
            foregroundVisibilityReporter: foregroundVisibilityReporter,
            audioSessionManager: serviceCoordinator.audioSessionManager,
            notificationsCleaner: notificationsCleaner,
            coinageService: serviceCoordinator.coinageService,
            networkStatusService: serviceCoordinator.networkStatusService,
            flowState: spaFlowState
        )
    }

    private static func createURLHandler(
        polkadotSignInService: PolkadotSignInService,
        storageFacade: StorageFacadeProtocol,
        serviceCoordinator: ServiceCoordinatorProtocol,
        flowState: ChatFlowState,
        moduleNavigator: ModuleNavigating,
        hostProvider: any ProductHostProviding
    ) -> URLHandlingService {
        let chatService = ChatOpenService(
            storageFacade: storageFacade,
            moduleNavigator: moduleNavigator,
            remoteContactResolver: RemoteContactOperationFactory()
        )
        #if FEATURE_SIGN_IN
            let signInHandler: [URLHandlingServiceProtocol] = [polkadotSignInService]
        #else
            let signInHandler: [URLHandlingServiceProtocol] = []
        #endif

        #if FEATURE_DIMS
            let dimHandlers: [URLHandlingServiceProtocol] = [
                DIM1OpenService(),
                DIM2OpenService(
                    serviceCoordinator: serviceCoordinator,
                    flowState: flowState.flowState
                ),
                GameChatService(flowState: flowState)
            ]
        #else
            let dimHandlers: [URLHandlingServiceProtocol] = []
        #endif

        let fiatOnrampRedirect = FiatOnrampRedirectService(
            fiatOnrampTransactionTracking: serviceCoordinator.fiatOnrampTrackingService
        )
        let payDeeplink = PayDeeplinkService(
            coinageService: serviceCoordinator.coinageService,
            moduleNavigator: moduleNavigator
        )
        #if FEATURE_PRODUCTS
            let productHandlers: [URLHandlingServiceProtocol] = [
                ProductSPAOpenService(
                    moduleNavigator: moduleNavigator,
                    hostProvider: hostProvider
                )
            ]
        #else
            let productHandlers: [URLHandlingServiceProtocol] = []
        #endif

        let historyStorage = W3sPaymentHistoryCoreDataStore(storageFacade: UserDataStorageFacade.shared)

        let w3sPayLauncher = W3sPayLauncher(
            coinageService: serviceCoordinator.coinageService,
            moduleNavigator: moduleNavigator,
            historyStore: historyStorage,
            logger: Logger.shared
        )
        let w3sPayDeeplink = W3sPayDeeplinkService(
            launcher: w3sPayLauncher,
            logger: Logger.shared
        )

        return URLHandlingService(children: signInHandler + [
            chatService
        ] + dimHandlers + [
            fiatOnrampRedirect,
            payDeeplink
        ] + productHandlers + [
            w3sPayDeeplink
        ])
    }

    @MainActor
    private static func createBrowserCoordinator(
        flowStateProvider: any SPAFlowStateProviding
    ) -> SPABrowserCoordinator {
        let tabManager = SPATabManager()
        return SPABrowserCoordinator(
            tabManager: tabManager,
            pool: SPAControllerPool(flowStateProvider: flowStateProvider)
        )
    }

    @MainActor
    private static func configurePresentationView(
        view: ControllerBackedProtocol,
        in serviceCoordinator: ServiceCoordinatorProtocol,
    ) {
        serviceCoordinator.accountManager.setPresentationView(view)
        serviceCoordinator.signInHostCoordinator.setPresentationView(view)
        serviceCoordinator.truapiRuntimeProvider.setPresentationView(view)
    }
}
