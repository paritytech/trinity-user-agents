import Foundation
import TrUAPIHost
import ChainRegistry
import SubstrateSdk
import Operation_iOS
import StructuredConcurrency

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
    private let chatFiles: NativeChatFilesHost
    private let workerManager: (any TrUAPIWorkerManaging)?
    private let logger: LoggerProtocol
    private weak var runtime: TrUAPIHostRuntime?

    init(
        chainRegistry: ChainRegistryProtocol,
        coreStorage: TrUAPILocalStoring,
        chainConnections: TrUAPIChainConnecting,
        confirmationPresenter: TrUAPIConfirmationPresenting,
        chatFiles: NativeChatFilesHost,
        workerManager: (any TrUAPIWorkerManaging)? = nil,
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.chainConnections = chainConnections
        self.confirmationPresenter = confirmationPresenter
        self.chatFiles = chatFiles
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

    func permissionAuthorizationsChanged(productId: String) {
        NotificationCenter.default.post(
            name: .productPermissionAuthorizationsChanged, object: productId
        )
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

    func allowedHopEndpoints(bulletinGenesisHash: Data) async throws -> [String] {
        chainConnections.allowedHopEndpoints(bulletinGenesisHash: bulletinGenesisHash)
    }

    func hopConnect(bulletinGenesisHash: Data, endpoint: String) throws -> UInt32? {
        try chainConnections.hopConnect(bulletinGenesisHash: bulletinGenesisHash, endpoint: endpoint)
    }

    func chainSend(connectionId: UInt32, request: String) throws {
        try chainConnections.send(connectionId: connectionId, request: request)
    }

    func chainClose(connectionId: UInt32) throws {
        chainConnections.close(connectionId: connectionId)
    }

    func pickChatFiles(request: NativeChatFilePickRequest) async throws -> [NativeChatPickedFile] {
        try await chatFiles.pickChatFiles(request: request)
    }

    func readChatFile(sourceId: String, offset: UInt64, length: UInt32) async throws -> Data {
        try await chatFiles.readChatFile(sourceId: sourceId, offset: offset, length: length)
    }

    func releaseChatFile(sourceId: String) async throws {
        try await chatFiles.releaseChatFile(sourceId: sourceId)
    }

    func beginChatFileExport(request: NativeChatFileExportRequest) async throws -> String? {
        try await chatFiles.beginChatFileExport(request: request)
    }

    func writeChatFileExport(exportId: String, offset: UInt64, data: Data) async throws {
        try await chatFiles.writeChatFileExport(exportId: exportId, offset: offset, data: data)
    }

    func finishChatFileExport(exportId: String) async throws {
        try await chatFiles.finishChatFileExport(exportId: exportId)
    }

    func cancelChatFileExport(exportId: String) async throws {
        try await chatFiles.cancelChatFileExport(exportId: exportId)
    }

    func confirmUserAction(review: UserConfirmationReview) async throws -> Bool {
        // TODO: pass the real SSO host identity once it is available at host level.
        await confirmationPresenter.confirm(review: review, from: "host")
    }

    func confirmPermission(review: UserConfirmationReview) async throws -> TrUAPIPermissionDecision {
        try await confirmationPresenter.confirmPermission(review: review, from: "host")
    }

    func identityUsernameCandidates(username: String, peopleChainGenesisHash: Data) async throws -> [Data] {
        let people = try chainRegistry.getChainOrError(for: AppConfig.Chains.usernameChain)
        guard let genesis = people.explicitGenesisHash,
              try Data(hexString: genesis) == peopleChainGenesisHash,
              AppConfigProvider.shared.getRemoteConfig()?.identityBackendUrl != nil else {
            throw HostRejection.Rejected(reason: "configured native identity service unavailable for this People chain")
        }
        // Reuse the native service's RemoteAppConfig URL, request model, JSON
        // decoder and JWT provider. No URL or credential is supplied by a guest.
        let tokenProvider = JWTTokenManager.shared
        let factory = UsernameOperationFactory(tokenProvider: tokenProvider)
        var cursor: String?
        var seenCursors = Set<String>()
        var candidates: [Data] = []
        // Search pagination follows the existing native backend cursor contract.
        // Refuse an unbounded/incomplete index rather than choose the first hit.
        for _ in 0..<32 {
            let wrapper = factory.createJWTAuthorizedRequestWrapper(
                endpoint: UsernameApi.V1.search(UsernameRequestModel(
                    prefix: username, caseSensitive: true, cursor: cursor
                )),
                responseFactory: UsernameJsonResultFactory<UsernameSearchResult>(),
                tokenProvider: tokenProvider
            )
            let result = try await wrapper.asyncExecute()
            candidates.append(contentsOf: try Self.identityCandidates(from: result, username: username))
            guard candidates.count <= 32 else {
                throw HostRejection.Rejected(reason: "too many native username candidates")
            }
            guard let next = result.nextCursor else { return candidates }
            guard seenCursors.insert(next).inserted else {
                throw HostRejection.Rejected(reason: "native username search repeated a cursor")
            }
            cursor = next
        }
        throw HostRejection.Rejected(reason: "native username search is incomplete")
    }

    static func identityCandidates(from result: UsernameSearchResult, username: String) throws -> [Data] {
        // The backend's prefix hits and status labels never establish ownership.
        // Core checks every exact-name AccountId32 against dotNS and People.
        let exact = result.usernames.filter { $0.username.value == username }
        guard exact.count <= 32 else {
            throw HostRejection.Rejected(reason: "too many native username candidates")
        }
        return try exact.map { candidate in
            let account = try candidate.accountId.toAccountId()
            guard account.count == 32 else {
                throw HostRejection.Rejected(reason: "native username candidate is not AccountId32")
            }
            return account
        }
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
