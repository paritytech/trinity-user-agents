import Foundation
import TrUAPIHost
import Products
import ChainRegistry
import Foundation_iOS
import FoundationExt
import SubstrateSdk
import BulletinChain
import DesignSystem

/// Opens app and widget executions on the shared host runtime.
struct RustRuntimeEnvironment {
    let runtime: TrUAPIHostRuntime
    let services: ProductExecutionServices

    /// The rust pieces a runtime needs: the opened execution and its chain
    /// connection pool for lifecycle control.
    struct ExecutionModel {
        let execution: TrUAPIProductExecutionProtocol
        let chainConnections: TrUAPIChainConnecting
        let osPermissionAsker: OSPermissionAsking

        /// Start the localhost ws-bridge and return the bootstrap script to
        /// inject. Called from the runtime's `start`; opening the execution
        /// stays side-effect free.
        /// The local session is activated once on the shared runtime, not here.
        func startBridge() throws -> String {
            let endpoint = try execution.startWsBridge(bindPort: 0)
            return LocalhostBridgeBootstrap.script(
                port: endpoint.port,
                token: endpoint.token
            )
        }
    }

    /// Open an SPA execution for `productId`. No ws-bridge start; that happens
    /// in the runtime's `start` via ``ExecutionModel/startBridge()``. The
    /// execution retains the bridge (callback retainer) and the bridge retains
    /// the pool, so holding `ExecutionModel` pins the whole chain.
    func makeSPAExecution(
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol
    ) throws -> ExecutionModel {
        try services.makeExecution(productId: productId, routers: routers, purpose: .app(runtime))
    }

    /// A Widget execution even for an app archive, so the page can call `ExpandedCard`.
    func makeExpandedCardExecution(
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol,
        card: any ExpandedCardCovering
    ) throws -> ExecutionModel {
        try services.makeExecution(productId: productId, routers: routers, purpose: .expandedCard(runtime, card: card))
    }
}

/// What one execution serves, which decides the bridge the core calls back on.
enum ExecutionPurpose {
    case app(TrUAPIHostRuntime)
    case expandedCard(TrUAPIHostRuntime, card: any ExpandedCardCovering)
    /// The product's one worker, serving its chat bot and its Pocket cards.
    case worker(chatMessaging: any ProductChatMessaging, attempt: NativeWorkerAttempt)
}

/// The iOS services shared by app, widget and worker executions.
struct ProductExecutionServices {
    let hostProvider: ProductHostProviding
    let gameReminders: ProductGameReminderScheduling?
    let logger: CoreEventLoggerProtocol

    private let chainRegistry = ChainRegistryFacade.sharedRegistry
    private let notificationScheduler = ProductNotificationScheduler.shared
    private let ipfsFetcher = IpfsFetcher(ipfsBaseURL: AppConfig.KnownIPFS.main)
    private let themeManager: ThemeManagerProtocol

    @MainActor
    init(
        hostProvider: ProductHostProviding,
        gameReminders: ProductGameReminderScheduling?,
        logger: CoreEventLoggerProtocol
    ) {
        self.hostProvider = hostProvider
        self.gameReminders = gameReminders
        self.logger = logger
        themeManager = ThemeManager.shared
    }

    func makeExecution(
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol,
        purpose: ExecutionPurpose
    ) throws -> RustRuntimeEnvironment.ExecutionModel {
        let chainConnections = TrUAPIV2SpecConnectionPool(
            chainRegistry: chainRegistry,
            applicationHandler: ApplicationHandler(),
            dateProvider: NowDateProvider(),
            logger: logger
        )

        let osPermissionAsker = OSPermissionAsker()
        let dependencies = makeBridgeDependencies(
            productId: productId,
            routers: routers,
            chainConnections: chainConnections,
            osPermissionAsker: osPermissionAsker
        )

        let bridges = purpose.makeBridges(dependencies: dependencies)

        let game = gameReminders == nil ? nil : bridges.host
        let execution: TrUAPIProductExecution =
            switch purpose {
            case let .worker(_, attempt):
                try attempt.openExecution(bridge: bridges.host, chat: bridges.chat, game: game)
            case let .app(runtime):
                runtime.openAppExecution(
                    bridge: bridges.host,
                    productId: productId,
                    chat: bridges.chat,
                    game: game
                )
            case let .expandedCard(runtime, _):
                runtime.openWidgetExecution(
                    bridge: bridges.host,
                    productId: productId,
                    chat: bridges.chat,
                    game: game
                )
            }

        bridges.host.attach(execution)

        return RustRuntimeEnvironment.ExecutionModel(
            execution: execution,
            chainConnections: chainConnections,
            osPermissionAsker: osPermissionAsker
        )
    }

    private func makeBridgeDependencies(
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol,
        chainConnections: TrUAPIChainConnecting,
        osPermissionAsker: OSPermissionAsker
    ) -> RustProductExecutionBridge.Dependencies {
        RustProductExecutionBridge.Dependencies(
            productId: productId,
            permissionGuard: ProductPermissionGuard.create(
                router: routers.productsRouter,
                osAsker: osPermissionAsker
            ),
            osPermissionAsker: osPermissionAsker,
            notificationScheduler: notificationScheduler,
            gameReminders: gameReminders,
            reminderPermissionAsker: osPermissionAsker,
            navigationRouter: routers.navigationRouter,
            chainRegistry: chainRegistry,
            chainConnections: chainConnections,
            productStorage: TrUAPILocalStorage.createProductLocalStorage(productId: productId),
            coreStorage: TrUAPILocalStorage.createCoreLocalStorage(),
            confirmationPresenter: TrUAPIConfirmationPresenter(routerFacade: routers),
            preimageCache: TrUAPIPreimageCache { [logger, ipfsFetcher] key in
                do {
                    return try await ipfsFetcher.lookupBy(rawHash: key)
                } catch {
                    logger.error("[truapi] preimage fetch failed for \(key.toHex()): \(error)")
                    return nil
                }
            },
            hostProvider: hostProvider,
            themeManager: themeManager,
            logger: logger
        )
    }
}

private extension ExecutionPurpose {
    /// A worker's bridge also answers its chat callbacks, so it is handed back twice.
    func makeBridges(
        dependencies: RustProductExecutionBridge.Dependencies
    ) -> (host: RustProductExecutionBridge, chat: RustChatExecutionBridge?) {
        switch self {
        case .app:
            return (RustProductExecutionBridge(dependencies: dependencies), nil)
        case let .expandedCard(_, card):
            return (RustProductExecutionBridge(dependencies: dependencies, card: card), nil)
        case let .worker(chatMessaging, _):
            let chat = RustChatExecutionBridge(dependencies: dependencies, chatMessaging: chatMessaging)
            return (chat, chat)
        }
    }
}
