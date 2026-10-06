import Foundation
import Products
import StructuredConcurrency
import TrUAPIHost
import UIKitExt

/// The chat half of a product's worker.
///
/// Started while a chat session with the product is open, which is what asks
/// ``TrUAPIWorkerManager`` for its worker. The core keeps one Worker execution
/// per product and decides when it runs, so this asks for one rather than
/// opening an execution of its own. The same worker also draws the product's
/// Pocket cards.
///
/// Chat-environment context route through that execution: user messages and
/// events publish chat actions, widget rendering streams typed renderer nodes,
/// and the product's chat surface serves the core's chat callbacks.
///
/// An actor so `start`/`dispose` never race on runtime state. Actors are
/// reentrant, so `dispose()` can interleave while `start` is suspended:
/// `dispose` flips `disposed` before its first await and `start` re-checks it
/// after every await, releasing anything it took in the gap.
actor TrUAPIChatHandler: ChatRuntimeProtocol {
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
    private let makeExecutionModel: @MainActor @Sendable (any ProductChatMessaging) throws
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

    /// How often a cell asks again while the product is still coming up. The
    /// worker's own refusals are waited out by ``renderWhenConnected``; this
    /// one covers the window before there is a worker to ask at all.
    private static let renderRetryInterval = Duration.milliseconds(25)

    /// What this session holds of the product's worker request, so dispose
    /// gives back exactly what start asked for and never more.
    private var request = WorkerRequest.none
    private var roomsForwardingTask: Task<Void, Never>?
    private var started = false
    private var disposed = false

    init(
        productUrl: URL,
        makeExecutionModel: @MainActor @Sendable @escaping (any ProductChatMessaging) throws
            -> RustRuntimeEnvironment.ExecutionModel,
        routers: ProductRoutersFacadeProtocol,
        engineFactory: @Sendable @escaping () -> JSEngineProtocol,
        renderStartupWindow: Duration = .seconds(5),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productId = productId
        self.workers = workers
        self.renderStartupWindow = renderStartupWindow
        self.logger = logger
        context = workers.context(of: productId)
    }

    deinit {
        // Locals first: assert's and &&'s autoclosures are nonisolated;
        // direct reads of the (Sendable) stored properties are only legal
        // in the deinit body itself.
        let started = started
        let disposed = disposed
        assert(!started || disposed, "TrUAPIChatHandler dropped without dispose()")
    }

    func start(messagingSupport: ProductsNativeApi.MessagingSupport) async throws {
        guard !started, !disposed else { throw CancellationError() }
        started = true

        do {
            try await startRuntime(messagingSupport: messagingSupport)
        } catch {
            // Nothing upstream tears us down — `ProductBot` only logs — so a
            // half-started session would hold its worker reference forever.
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
        context.routers.setPresentationView(view)
    }

    func dispose() async {
        guard !disposed else { return }
        // Flipped before the first suspension: any start resuming after this
        // point observes it and unwinds.
        disposed = true

        roomsForwardingTask?.cancel()
        roomsForwardingTask = nil

        // The core keeps the bridge, and the bridge keeps the surface: unbinding
        // is what releases the chat context. Only this runtime's own binding,
        // because the surface is the product's and another runtime may hold it.
        context.chat.unbind(owner: self)

        await executionModel?.media.close()
        await destroyEngineResources()

        if let executionModel {
            await executionModel.close()
        }

        logger.debug("Rust chat runtime disposed for: \(productUrl)")
    }
}

private extension TrUAPIChatHandler {
    func startRuntime(
        messagingSupport: ProductsNativeApi.MessagingSupport
    ) async throws {
        // Bound before the worker is asked for, so the core can never reach a
        // surface with no binding.
        context.chat.bind(messagingSupport, owner: self)

        let model = try await makeExecutionModel(chatSurface)
        guard !disposed else {
            await model.close()
            throw CancellationError()
        }
        executionModel = model
        startRoomsForwarding(chatMessaging: chatSurface, execution: model.execution)

        let bootstrapScript = try model.startBridge()
        let scriptsFactory = RustRuntimeScriptsFactory(bootstrapScript: bootstrapScript)
        let jsEngine = try await bootEngine(
            scripts: scriptsFactory.makeScripts(),
            media: model.media
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
        media: NativeMediaBackend
    ) async throws -> JSEngineProtocol {
        let jsEngine = engineFactory()
        do {
            await jsEngine.registerJSDeviceCapabilityHandler { _ in .denied }
            try checkNotDisposed()
            try await jsEngine.initialize(with: scripts)
            guard await jsEngine.getState() == .ready else {
                throw ScriptExecutorError.engineInitFailed
            }
            try checkNotDisposed()
        } catch {
            settleRequest()
            throw error
        }
        await media.watchEngine {
            switch await jsEngine.getState() {
            case .destroyed, .error: return false
            default: return true
            }
        }
        guard !disposed else {
            await jsEngine.destroy()
            throw CancellationError()
        }

        try checkNotDisposed()
        startRoomsForwarding()

        logger.debug("Rust chat runtime started for: \(productId)")
    }

    /// The ask has returned, so the request is ours either way: it is
    /// registered before the wait that can fail. A dispose that landed while we
    /// were asking left the giving back to here.
    func settleRequest() {
        request = .held

        if disposed { releaseWorker() }
    }

    func releaseWorker() {
        guard request == .held else { return }

        request = .none
        workers.releaseWorker(for: productId)
    }

    func checkNotDisposed() throws {
        guard !disposed else { throw CancellationError() }
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
                return try await requireExecution().renderWhenConnected(
                    request,
                    until: deadline,
                    retryEvery: Self.renderRetryInterval
                )
            } catch let error where (error as? ChatSeamError) == .notStarted {
                guard ContinuousClock.now < deadline else {
                    logger.error("Custom render gave up waiting for the product: \(messageId)")
                    throw error
                }
                try await Task.sleep(for: Self.renderRetryInterval)
            }
        }
    }

    func requireExecution() throws -> TrUAPIProductExecutionProtocol {
        guard let execution = workers.currentExecution(of: productId) else {
            throw ChatSeamError.notStarted
        }
        return execution
    }

    /// Mirror the native room list into the core so product-side
    /// `chat.listSubscribe` sees native changes as they happen. The execution is
    /// read each time rather than captured, so a worker that restarts under this
    /// session keeps being told.
    func startRoomsForwarding() {
        roomsForwardingTask = Task { [logger, workers, productId, chat = context.chat] in
            do {
                for try await rooms in try await chat.subscribeRooms() {
                    guard !Task.isCancelled else { return }
                    workers.currentExecution(of: productId)?
                        .notifyChatRoomsChanged(rooms: rooms.map { $0.toChatRoom() })
                }
            } catch {
                guard !Task.isCancelled else { return }
                logger.error("Rust chat runtime rooms forwarding ended: \(error)")
            }
        }
    }
}
