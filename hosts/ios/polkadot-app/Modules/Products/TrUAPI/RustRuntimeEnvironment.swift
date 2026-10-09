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
        /// (``makeSPAExecution``/``makeChatExecution``) stays side-effect free.
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
    func makeSPAExecution(productId: ProductId, routers: ProductRoutersFacadeProtocol) throws -> ExecutionModel {
        try makeExecution(productId: productId, routers: routers, kind: .app)
    }

    /// Open a chat execution for `productId`. Mirrors ``makeSPAExecution``.
    func makeChatExecution(
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol,
        chatMessaging: any ProductChatMessaging
    ) throws -> ExecutionModel {
        try makeExecution(productId: productId, routers: routers, kind: .worker, chatMessaging: chatMessaging)
    }
}

private extension RustRuntimeEnvironment {
    func makeExecution(
        productId: ProductId,
        routers: ProductRoutersFacadeProtocol,
        kind: ProductExecutionKind,
        chatMessaging: (any ProductChatMessaging)? = nil
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

        let chatBridge = chatMessaging.map {
            RustChatExecutionBridge(dependencies: dependencies, chatMessaging: $0)
        }
        let bridge = chatBridge ?? RustProductExecutionBridge(dependencies: dependencies)

        let execution = try runtime.openProductExecution(
            bridge: bridge,
            configuration: ProductExecutionConfig(productId: productId, executionKind: kind),
            chat: chatBridge,
            game: gameReminders == nil ? nil : bridge
        )

        bridge.attach(execution)

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
