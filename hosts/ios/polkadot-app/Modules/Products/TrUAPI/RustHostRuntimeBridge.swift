import Foundation
import TrUAPIHost
import ChainRegistry
import SubstrateSdk

/// `HostBridge` for the process-wide ``TrUAPIHostRuntime``. It has no product
/// identity: it owns the host-global core storage, host-level chain access, and
/// the answers the runtime needs before any product opens. Product-scoped
/// capabilities (navigation, permissions, notifications, product KV) live in
/// the per-execution ``RustProductExecutionBridge``; here they deny or fall to
/// the `HostBridge` protocol defaults.
///
/// Threading contract matches `HostBridge`: sync members run inline on the
/// dispatcher thread and return promptly.
final class RustHostRuntimeBridge: HostBridge, @unchecked Sendable {
    let storage: HostStorageBackend
    let coreStorage: HostCoreStorageBackend

    private let chainRegistry: ChainRegistryProtocol
    private let chainConnections: TrUAPIChainConnecting
    private let confirmationPresenter: TrUAPIConfirmationPresenting
    private let workerManager: (any TrUAPIWorkerManaging)?
    private let logger: LoggerProtocol
    private weak var runtime: TrUAPIHostRuntime?

    init(
        chainRegistry: ChainRegistryProtocol,
        coreStorage: TrUAPILocalStoring,
        chainConnections: TrUAPIChainConnecting,
        confirmationPresenter: TrUAPIConfirmationPresenting,
        workerManager: (any TrUAPIWorkerManaging)? = nil,
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.chainConnections = chainConnections
        self.confirmationPresenter = confirmationPresenter
        self.workerManager = workerManager
        self.logger = logger
        self.coreStorage = CoreStorageBackend(storage: coreStorage)
        storage = EmptyHostStorageBackend()
        chainConnections.eventHandler = self
    }

    /// Attach the runtime that owns this bridge so native chain events can
    /// notify it in place. Called once, right after the runtime is built.
    func attach(_ runtime: TrUAPIHostRuntime) {
        self.runtime = runtime
    }

    func onCoreLog(marker: String, detail: String) {
        logger.debug("[truapi:host:\(marker)] \(detail)")
    }

    /// Demand is runtime-wide, so it arrives here rather than on a product's
    /// own bridge, and can arrive re-entrantly from inside `acquireWorker`,
    /// the manager hands the transition off rather than acting on it here.
    func workerDemandChanged(productId: String, transition: WorkerTransition) {
        workerManager?.demandChanged(productId: productId, transition: transition)
    }

    func navigateTo(url: String) async throws {
        throw HostNavigateToError.Unknown(reason: "navigation unavailable at host level: \(url)")
    }

    func devicePermission(
        product _: ProductExecutionConfig,
        request _: HostDevicePermissionRequest
    ) async throws -> TrUAPIPermissionDecision {
        .deny
    }

    func remotePermission(
        product _: ProductExecutionConfig,
        request _: RemotePermission
    ) async throws -> TrUAPIPermissionDecision {
        .deny
    }

    func chainConnect(genesisHash: Data) throws -> UInt32? {
        chainConnections.connect(genesisHash: genesisHash)
    }

    func chainSend(connectionId: UInt32, request: String) throws {
        try chainConnections.send(connectionId: connectionId, request: request)
    }

    func chainClose(connectionId: UInt32) throws {
        chainConnections.close(connectionId: connectionId)
    }

    func confirmUserAction(review: UserConfirmationReview) async throws -> Bool {
        // TODO: pass the real SSO host identity once it is available at host level.
        await confirmationPresenter.confirm(review: review, from: "host")
    }

    func confirmPermission(review: UserConfirmationReview) async throws -> TrUAPIPermissionDecision {
        await confirmationPresenter.confirmPermission(review: review, from: "host")
    }

    func featureSupported(request: HostFeatureSupportedRequest) async throws -> Bool {
        switch request {
        case let .chain(genesisHash):
            // Aligned with chain_connect: supported means the app holds a live
            // connection, not merely a registry entry.
            guard let chain = chainRegistry.getChainByGenesis(for: genesisHash.toHex()) else {
                return false
            }
            return chainRegistry.getConnection(for: chain.chainId) != nil
        }
    }

    func supportedChains() throws -> HostChainSet {
        TrUAPISupportedChains.make(chainRegistry: chainRegistry)
    }

    func authStateChanged(state: AuthState) {
        let detail =
            switch state {
            case .disconnected: "disconnected"
            case .pairing: "pairing"
            case .connected: "connected"
            case .loginFailed: "login failed"
            case .authenticating: "authenticating"
            }

        logger.debug("[truapi:host] auth state: \(detail)")
    }
}

// MARK: - Notify-back event handlers

extension RustHostRuntimeBridge: TrUAPIChainEventHandling {
    func chainDidReceiveResponse(connectionId: UInt32, json: String) {
        runtime?.notifyChainResponse(connectionId: connectionId, json: json)
    }

    func chainDidClose(connectionId: UInt32) {
        runtime?.notifyChainClosed(connectionId: connectionId)
    }
}
