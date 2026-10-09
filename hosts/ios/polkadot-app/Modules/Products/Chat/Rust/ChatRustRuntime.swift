import Foundation
import Products
import TrUAPIHost
import UIKitExt

/// Rust chat runtime: the TrUAPI core handles product requests over its
/// localhost ws-bridge. Chat-environment seams route through the core:
/// user messages and events publish chat actions, widget rendering streams
/// typed renderer nodes, and the chat surface serves the core's chat callbacks
/// via the execution's ``RustChatExecutionBridge``.
///
/// An actor so `start`/`dispose` never race on runtime state. Actors are
/// reentrant, so `dispose()` can interleave while `start` is suspended:
/// `dispose` flips `disposed` before its first await and `start` re-checks
/// it after every await, destroying anything it created in the gap.
actor ChatRustRuntime: ChatRuntimeProtocol {
    enum ChatSeamError: Error, Equatable {
        case notStarted
        /// The core normalizes room ids on the way back and rejects an empty one, so a
        /// chat with no room would reach the product as a message it cannot answer.
        case roomlessChat
        /// The stored message carries no product-defined type, so no
        /// `RenderContext` can name the body the action came from.
        case untypedBody
    }

    private let productUrl: URL
    /// Opened in `start`, not in `init`: the core's worker registry is keyed by
    /// product id and evicts the previous entry, so a bot rebuilt before it
    /// starts would close the execution the live one is about to use.
    private let makeExecutionModel: @Sendable (any ProductChatMessaging) throws
        -> RustRuntimeEnvironment.ExecutionModel
    private var executionModel: RustRuntimeEnvironment.ExecutionModel?
    /// Bound for as long as the chat surface is alive, like the shared worker's
    /// native api.
    private let chatSurface = ProductChatSurface()
    // Set once in init and only read from the MainActor-isolated `attach`;
    // all facade mutation happens behind its own @MainActor method.
    private nonisolated(unsafe) let routers: ProductRoutersFacadeProtocol
    private let engineFactory: @Sendable () -> JSEngineProtocol
    private let renderStartupWindow: Duration
    private let logger: LoggerProtocol

    private var engine: JSEngineProtocol?
    private var engineMonitor: JSEngineMonitor?
    private var revocationObserver: ProductExecutionRevocationObserver?
    private var moduleBridge: JSESModuleBridge?
    private var roomsForwardingTask: Task<Void, Never>?
    private var started = false
    private var disposed = false

    init(
        productUrl: URL,
        makeExecutionModel: @Sendable @escaping (any ProductChatMessaging) throws
            -> RustRuntimeEnvironment.ExecutionModel,
        routers: ProductRoutersFacadeProtocol,
        engineFactory: @Sendable @escaping () -> JSEngineProtocol,
        renderStartupWindow: Duration = .seconds(5),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productUrl = productUrl
        self.makeExecutionModel = makeExecutionModel
        self.routers = routers
        self.engineFactory = engineFactory
        self.renderStartupWindow = renderStartupWindow
        self.logger = logger
    }

    deinit {
        // Locals first: assert's and &&'s autoclosures are nonisolated;
        // direct reads of the (Sendable) stored properties are only legal
        // in the deinit body itself.
        let started = started
        let disposed = disposed
        assert(!started || disposed, "ChatRustRuntime dropped without dispose()")
    }

    func start(messagingSupport: ProductsNativeApi.MessagingSupport) async throws {
        guard !started, !disposed else { throw CancellationError() }
        started = true

        do {
            try await startRuntime(messagingSupport: messagingSupport)
        } catch {
            // Nothing upstream tears us down — `ProductBot` only logs — so a
            // half-built runtime would keep its engine, socket and pool alive.
            await dispose()
            throw error
        }
    }

    func onUserMessage(text: String, roomId: String?) async throws {
        try checkNotDisposed()
        guard let roomId else { throw ChatSeamError.roomlessChat }
        try requireExecution().publishChatAction(HostChatActionSubscribeItem(
            roomId: roomId,
            peer: "native",
            payload: .messagePosted(.text(text: text))
        ))
    }

    func renderMessage(
        roomId: String?,
        messageId: String,
        messageType: String,
        messageData: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        do {
            guard let roomId else { throw ChatSeamError.roomlessChat }
            let nodes = try await renderNodesWhenConnected(
                deadline: ContinuousClock.now + renderStartupWindow,
                roomId: roomId,
                messageId: messageId,
                messageType: messageType,
                messageData: messageData
            )
            return AsyncThrowingStream { continuation in
                let task = Task {
                    do {
                        for try await node in nodes {
                            continuation.yield(.native(node))
                        }
                        continuation.finish()
                    } catch {
                        continuation.finish(throwing: error)
                    }
                }
                continuation.onTermination = { _ in task.cancel() }
            }
        } catch {
            return AsyncThrowingStream { $0.finish(throwing: error) }
        }
    }

    /// A press inside a body the product drew is a renderer action addressed by
    /// `RenderContext`, not a chat action: `ChatActionPayload.actionTriggered`
    /// means a host-drawn `Actions` button, which this host does not raise.
    func dispatchEvent(
        roomId: String?,
        messageId: String,
        messageType: String?,
        actionId: String,
        payload: String?
    ) async {
        do {
            try checkNotDisposed()
            guard let roomId else { throw ChatSeamError.roomlessChat }
            // The core routes by context, so an action whose body we cannot name
            // would be delivered nowhere. Only this runtime needs the type: the
            // native one addresses by message id.
            guard let messageType else { throw ChatSeamError.untypedBody }
            try requireExecution().publishRendererAction(HostRendererActionSubscribeItem(
                context: .chatMessage(
                    roomId: roomId,
                    messageId: messageId,
                    messageType: messageType
                ),
                actionId: actionId,
                payload: payload.map { Data($0.utf8) } ?? Data()
            ))
        } catch is CancellationError {
            logger.debug("Rust chat runtime disposed before event \(actionId)")
        } catch {
            logger.error("Rust chat runtime failed to dispatch event \(actionId): \(error)")
        }
    }

    @MainActor
    func attach(presentationView view: ControllerBackedProtocol) {
        routers.setPresentationView(view)
    }

    func dispose() async {
        guard !disposed else { return }
        // Flipped before the first suspension: any start resuming after this
        // point observes it and unwinds.
        disposed = true
        revocationObserver = nil

        roomsForwardingTask?.cancel()
        roomsForwardingTask = nil

        // The core keeps the bridge, and the bridge keeps the surface: unbinding
        // is what releases the chat context.
        chatSurface.unbind()

        await destroyEngineResources()

        if let executionModel {
            executionModel.execution.stopWsBridge()
            executionModel.execution.close()
            executionModel.chainConnections.closeAll()
        }

        logger.debug("Rust chat runtime disposed for: \(productUrl)")
    }
}

private extension ChatRustRuntime {
    func startRuntime(
        messagingSupport: ProductsNativeApi.MessagingSupport
    ) async throws {
        // Bound before the bridge starts, so the core can never reach a surface
        // with no binding.
        chatSurface.bind(messagingSupport)

        let model = try makeExecutionModel(chatSurface)
        executionModel = model
        revocationObserver = ProductExecutionRevocationObserver(execution: model.execution) { [weak self] in
            await self?.dispose()
        }
        startRoomsForwarding(chatMessaging: chatSurface, execution: model.execution)

        let bootstrapScript = try model.startBridge()
        let scriptsFactory = RustRuntimeScriptsFactory(bootstrapScript: bootstrapScript)
        let jsEngine = try await bootEngine(
            scripts: scriptsFactory.makeScripts(),
            osPermissionAsker: model.osPermissionAsker
        )
        try checkNotDisposed()

        let modBridge = JSESModuleBridge(engine: jsEngine)
        await modBridge.install()

        try checkNotDisposed()
        moduleBridge = modBridge

        try await modBridge.executeScript(url: productUrl)

        logger.debug("Rust chat runtime started for: \(productUrl)")
    }

    func bootEngine(
        scripts: [JSEngineScript],
        osPermissionAsker: OSPermissionAsking
    ) async throws -> JSEngineProtocol {
        let jsEngine = engineFactory()
        do {
            await jsEngine.registerJSDeviceCapabilityHandler(
                osPermissionAsker.makeDeviceCapabilityHandler()
            )
            try checkNotDisposed()
            try await jsEngine.initialize(with: scripts)
            guard await jsEngine.getState() == .ready else {
                throw ScriptExecutorError.engineInitFailed
            }
            try checkNotDisposed()
        } catch {
            await jsEngine.destroy()
            throw error
        }

        let monitor = JSEngineMonitor(
            engine: jsEngine,
            pauseEvent: .willResignActive,
            resumeEvent: .didBecomeActive
        )
        monitor.start()
        engineMonitor = monitor

        engine = jsEngine
        return jsEngine
    }

    func destroyEngineResources() async {
        engineMonitor?.stop()
        engineMonitor = nil

        let moduleBridge = moduleBridge
        self.moduleBridge = nil
        await moduleBridge?.dispose()

        let engine = engine
        self.engine = nil
        await engine?.destroy()
    }

    func checkNotDisposed() throws {
        guard !disposed, executionModel?.execution.isClosed() != true else { throw CancellationError() }
    }

    /// A persisted message can decode before the product attaches. `ProductMessageDecoder`
    /// never evicts, so failing once breaks that cell for the session.
    func renderNodesWhenConnected(
        deadline: ContinuousClock.Instant,
        roomId: String,
        messageId: String,
        messageType: String,
        messageData: Data
    ) async throws -> AsyncThrowingStream<RendererNode, Error> {
        let request = ProductRendererRenderRequest(
            context: .chatMessage(
                roomId: roomId,
                messageId: messageId,
                messageType: messageType
            ),
            payload: messageData
        )

        while true {
            try checkNotDisposed()
            do {
                return try requireExecution().render(request)
            } catch let error where error.isTransientRenderStartupError {
                guard ContinuousClock.now < deadline else {
                    logger.error("Custom render gave up waiting for the product: \(messageId)")
                    throw error
                }
                try await Task.sleep(for: .milliseconds(25))
            }
        }
    }

    func requireExecution() throws -> TrUAPIProductExecutionProtocol {
        guard let executionModel else { throw ChatSeamError.notStarted }
        return executionModel.execution
    }

    /// Mirror the native room list into the core so product-side
    /// `chat.listSubscribe` sees native changes as they happen.
    func startRoomsForwarding(
        chatMessaging: any ProductChatMessaging,
        execution: TrUAPIProductExecutionProtocol
    ) {
        roomsForwardingTask = Task { [logger] in
            do {
                for try await rooms in try await chatMessaging.subscribeRooms() {
                    guard !Task.isCancelled else { return }
                    execution.notifyChatRoomsChanged(rooms: rooms.map { $0.toChatRoom() })
                }
            } catch {
                guard !Task.isCancelled else { return }
                logger.error("Rust chat runtime rooms forwarding ended: \(error)")
            }
        }
    }
}

private extension Error {
    /// A cell can render before `start` opens the execution (`notStarted`) or before
    /// the product attaches (`NotConnected`); the retry waits both out. Everything
    /// else surfaces at once. Only covers synchronous throws — a failure delivered
    /// inside the node stream never reaches here.
    var isTransientRenderStartupError: Bool {
        if (self as? ProductRuntimeError) == .NotConnected { return true }
        return (self as? ChatRustRuntime.ChatSeamError) == .notStarted
    }
}
