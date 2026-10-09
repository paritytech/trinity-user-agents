import Foundation
import TrUAPIHost
import Products
import ChainRegistry
import SubstrateSdk
import BulletinChain

/// Shared dependencies for opening one product execution off the process-wide
/// ``TrUAPIHostRuntime``. Embedded by both the SPA and chat rust runtime
/// factories. Session activation lives on the shared runtime, so no session
/// data is held here.
struct RustRuntimeEnvironment {
    let runtime: TrUAPIHostRuntime
    let chainRegistry: ChainRegistryProtocol
    let notificationScheduler: ProductNotificationScheduling
    let gameReminders: ProductGameReminderScheduling?
    let ipfsFetcher: IpfsFetching
    let hostProvider: ProductHostProviding
    let logger: LoggerProtocol

    /// The rust pieces a runtime needs: the opened execution and its chain
    /// connection pool for lifecycle control.
    struct ExecutionModel {
        let execution: TrUAPIProductExecutionProtocol
        let chainConnections: TrUAPIChainConnecting
        let osPermissionAsker: OSPermissionAsking

        /// Start the localhost ws-bridge and return the bootstrap script to
        /// inject. Called from the runtime's `start`; opening the execution
        /// (``makeSPAExecution``/``makeWorkerExecution``) stays side-effect free.
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
        routers: ProductRoutersFacadeProtocol,
        cardFace: (any ExpandedCardFaceShowing)?
    ) throws -> ExecutionModel {
        try makeExecution(productId: productId, routers: routers, purpose: .page(cardFace: cardFace))
    }

    /// Open `productId`'s one Worker execution. The core keeps a single Worker
    /// execution per product and closes the previous one when another opens, so
    /// every modality is served from this one: both bridges are handed over
    /// here rather than attached afterwards, because the worker may list its
    /// cards or post a chat message as soon as it connects.
    func makeWorkerExecution(
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol,
        chatMessaging: any ProductChatMessaging,
        pocket: any PocketHostBridge
    ) throws -> ExecutionModel {
        try makeExecution(
            productId: productId,
            routers: routers,
            purpose: .worker(chatMessaging: chatMessaging, pocket: pocket)
        )
    }
}

/// What one execution serves, which decides the bridge the core calls back on.
enum ExecutionPurpose {
    /// A product's page, under the face of the Pocket card it was opened from, if any.
    case page(cardFace: (any ExpandedCardFaceShowing)?)
    /// The product's one worker, serving its chat bot and its Pocket cards.
    case worker(chatMessaging: any ProductChatMessaging, pocket: any PocketHostBridge)
}

private extension RustRuntimeEnvironment {
    func makeExecution(
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol,
        purpose: ExecutionPurpose
    ) throws -> ExecutionModel {
        let chainConnections = TrUAPIChainConnectionPool(
            engineResolver: { [chainRegistry] genesisHash in
                chainRegistry.getChainByGenesis(for: genesisHash.toHex()).flatMap { chain in
                    chainRegistry.getConnection(for: chain.chainId)
                }
            },
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

        let execution = try runtime.openProductExecution(
            bridge: bridges.host,
            configuration: ProductExecutionConfig(productId: productId, executionKind: purpose.executionKind),
            chat: bridges.chat,
            pocket: purpose.pocket,
            game: gameReminders == nil ? nil : bridges.host
        )

        bridges.host.attach(execution)

        return ExecutionModel(
            execution: execution,
            chainConnections: chainConnections,
            osPermissionAsker: osPermissionAsker
        )
    }

    func makeBridgeDependencies(
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol,
        chainConnections: TrUAPIChainConnecting,
        osPermissionAsker: OSPermissionAsker
    ) -> RustProductExecutionBridge.Dependencies {
        RustProductExecutionBridge.Dependencies(
            productId: productId,
            permissionGuard: ProductPermissionGuard.create(
                router: routers.productsRouter,
                fundingProvider: FundingDomainProvider(hostProvider: hostProvider),
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
            logger: logger
        )
    }
}

extension ExecutionPurpose {
    var executionKind: ProductExecutionKind {
        switch self {
        case let .page(cardFace): cardFace == nil ? .app : .widget
        case .worker: .worker
        }
    }
}

private extension ExecutionPurpose {
    var pocket: (any PocketHostBridge)? {
        guard case let .worker(_, pocket) = self else { return nil }

        return pocket
    }

    /// A worker's bridge also answers its chat callbacks, so it is handed back twice.
    func makeBridges(
        dependencies: RustProductExecutionBridge.Dependencies
    ) -> (host: RustProductExecutionBridge, chat: RustChatExecutionBridge?) {
        switch self {
        case let .page(cardFace):
            return (RustProductExecutionBridge(dependencies: dependencies, cardFace: cardFace), nil)
        case let .worker(chatMessaging, _):
            let chat = RustChatExecutionBridge(dependencies: dependencies, chatMessaging: chatMessaging)
            return (chat, chat)
        }
    }
}
