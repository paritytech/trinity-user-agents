import Foundation
import Products
import StructuredConcurrency
import TrUAPIHost
import UIKitExt

/// The chat half of a product's worker.
///
/// Started when the chat bot of the product is discovered. The start binds the chat surface of the product and
/// holds no worker (02-DESIGN D10). Each open room screen and each query asks ``TrUAPIWorkerManager`` for the
/// worker, and gives the request back when it ends. The core keeps one Worker execution per product and decides
/// when it runs, so this asks for one rather than opening an execution of its own. The same worker also draws the
/// product's Pocket cards.
///
/// Chat-environment context route through that execution: what the user does
/// in a room goes out as a Chat Extension v2 event, the renderer of a widget
/// streams typed nodes, and the product's chat surface serves the core's chat
/// callbacks.
///
/// An actor so `start`/`dispose` never race on runtime state. Actors are
/// reentrant, so `dispose()` can interleave while `start` is suspended:
/// `dispose` flips `disposed` before its first await and `start` re-checks it
/// after every await, releasing anything it took in the gap.
actor TrUAPIChatHandler: ChatRuntimeProtocol, ProductChatQuerying, ChatRoomLifecycle {
    enum ChatSeamError: Error, Equatable {
        case notStarted
        /// The core normalizes room ids on the way back and rejects an empty one, so a
        /// chat with no room would reach the product as a message it cannot answer.
        case roomlessChat
        /// The stored message carries no product-defined type, so no
        /// `RenderContext` can name the body the action came from.
        case untypedBody
    }

    private let productId: ProductId
    private let workers: any TrUAPIWorkerManaging
    /// The product's own chat surface and routers, which outlive any one worker.
    private let context: ProductWorkerContext
    private let renderStartupWindow: Duration
    private let activityStore: ChatRoomActivityStoring
    private let logger: LoggerProtocol

    /// How often a cell asks again while the product is still coming up. The
    /// worker's own refusals are waited out by ``renderWhenConnected``; this
    /// one covers the window before there is a worker to ask at all.
    private static let renderRetryInterval = Duration.milliseconds(25)

    /// What the harness of the simulator holds of the product's worker request, so dispose
    /// gives back exactly what ``holdWorker()`` asked for and never more.
    private var request = WorkerRequest.none
    /// The worker requests of the open room screens, by room. Each open that does not throw adds one, and each
    /// close gives one back.
    private var openRooms: [String: Int] = [:]
    private var roomsForwardingTask: Task<Void, Never>?
    private var started = false
    private var disposed = false

    init(
        productId: ProductId,
        workers: any TrUAPIWorkerManaging,
        renderStartupWindow: Duration = .seconds(5),
        activityStore: ChatRoomActivityStoring = ChatRoomActivityStore.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productId = productId
        self.workers = workers
        self.renderStartupWindow = renderStartupWindow
        self.activityStore = activityStore
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

    /// Binds the chat surface of the product, so the core can reach it whenever the worker runs. It holds no
    /// worker: the worker starts when a room screen opens (``open(roomId:)``) or a query is due.
    func start(messagingSupport: ProductsNativeApi.MessagingSupport) async throws {
        guard !started, !disposed else { throw CancellationError() }
        started = true

        context.chat.bind(messagingSupport, owner: self)
        startRoomsForwarding()

        logger.debug("Rust chat runtime started for: \(productId)")
    }

    /// Holds the worker until ``dispose()``. Only the harness of the simulator calls it, so its command feed
    /// reaches the product while no room screen is open.
    func holdWorker() async throws {
        try checkNotDisposed()
        guard request == .none else { return }

        request = .asking
        do {
            _ = try await workers.ensureWorker(for: productId)
        } catch {
            settleRequest()
            releaseWorker()
            throw error
        }
        settleRequest()
    }

    // MARK: - ChatRoomLifecycle

    /// Holds the worker for the open room screen, then sends `RoomOpened` (02-DESIGN §8.4). Throws when the
    /// worker does not start in the startup window of the worker manager. Then the room holds nothing.
    func open(roomId: String) async throws {
        try checkNotDisposed()

        do {
            _ = try await workers.ensureWorker(for: productId)
        } catch {
            workers.releaseWorker(for: productId)
            throw error
        }

        guard !disposed else {
            workers.releaseWorker(for: productId)
            throw CancellationError()
        }

        openRooms[roomId, default: 0] += 1
        publishRoomEvent(.roomOpened, roomId: roomId)
    }

    /// Sends `RoomClosed`, clears the activity line of the room, and gives back the worker request of the open.
    func close(roomId: String) async {
        guard let count = openRooms[roomId], count > 0 else { return }

        openRooms[roomId] = count > 1 ? count - 1 : nil
        publishRoomEvent(.roomClosed, roomId: roomId)
        activityStore.set(nil, for: .chatExtension(productId, roomId: roomId))
        workers.releaseWorker(for: productId)
    }

    // MARK: - ProductChatQuerying

    func messagesPage(_ request: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryItem {
        try await withWorker { execution in
            try await execution.chatMessagesQuery(request, timeout: ChatQueryTimeout.messages)
        }
    }

    func userAction(_ request: ProductChatUserActionRequest) async throws -> ProductChatUserActionItem {
        try await withWorker { execution in
            try await execution.chatUserAction(request, timeout: ChatQueryTimeout.timeout(for: request.action))
        }
    }

    /// Runs `body` with the execution of the worker, and holds the worker while it runs (02-DESIGN D10). A worker
    /// that does not run starts first.
    func withWorker<Answer: Sendable>(
        _ body: @Sendable (TrUAPIProductExecutionProtocol) async throws -> Answer
    ) async throws -> Answer {
        try checkNotDisposed()

        let execution: TrUAPIProductExecutionProtocol
        do {
            execution = try await workers.ensureWorker(for: productId)
        } catch {
            workers.releaseWorker(for: productId)
            throw error
        }
        defer { workers.releaseWorker(for: productId) }

        return try await body(execution)
    }

    func onUserMessage(text: String, roomId: String?) async throws {
        try checkNotDisposed()
        guard let roomId else { throw ChatSeamError.roomlessChat }
        try requireExecution().publishChatAction(HostChatActionSubscribeItem(
            roomId: roomId,
            peer: Self.hostUserPeer,
            payload: .messagePosted(.text(text: text))
        ))
    }

    func publishUserEvent(_ event: ProductChatUserEvent, roomId: String?) async throws {
        try checkNotDisposed()
        guard let roomId else { throw ChatSeamError.roomlessChat }
        try requireExecution().publishChatEvent(roomId: roomId, peer: Self.hostUserPeer, event: event.chatEvent)
    }

    func renderMessage(
        roomId: String?,
        messageId: String,
        messageType: String,
        messageData: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        do {
            guard let roomId else { throw ChatSeamError.roomlessChat }
            let request = ProductRendererRenderRequest(
                context: .chatMessage(
                    roomId: roomId,
                    messageId: productMessageId(messageId, roomId: roomId),
                    messageType: messageType
                ),
                payload: messageData
            )
            return try await renderOutputs(request, name: messageId)
        } catch {
            return AsyncThrowingStream { $0.finish(throwing: error) }
        }
    }

    /// The tree of the custom footer of a room (`ChatRoomFooter::Custom`), drawn through the renderer with a
    /// `ChatFooter` context and an empty payload (02-DESIGN §6.4, §13 item 14).
    func renderFooter(roomId: String) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        do {
            let request = ProductRendererRenderRequest(context: .chatFooter(roomId: roomId), payload: Data())
            return try await renderOutputs(request, name: "footer of \(roomId)")
        } catch {
            return AsyncThrowingStream { $0.finish(throwing: error) }
        }
    }

    /// A press inside the custom footer reaches `Renderer::action_subscribe` with the `ChatFooter` context.
    func dispatchFooterAction(roomId: String, actionId: String, payload: String?) async {
        do {
            try checkNotDisposed()
            try requireExecution().publishRendererAction(HostRendererActionSubscribeItem(
                context: .chatFooter(roomId: roomId),
                actionId: actionId,
                payload: payload.map { Data($0.utf8) } ?? Data()
            ))
        } catch {
            logger.error("Rust chat runtime failed to dispatch footer action \(actionId): \(error)")
        }
    }

    /// A press inside a body the product drew is a renderer action addressed by
    /// `RenderContext`, not a chat action: `ChatActionPayload.actionTriggered`
    /// means a host-drawn `Actions` button, see ``dispatchActionTrigger(roomId:messageId:actionId:)``.
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
                    messageId: productMessageId(messageId, roomId: roomId),
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

    func dispatchActionTrigger(roomId: String?, messageId: String, actionId: String) async {
        do {
            try checkNotDisposed()
            guard let roomId else { throw ChatSeamError.roomlessChat }
            let trigger = ActionTrigger(
                messageId: productMessageId(messageId, roomId: roomId),
                actionId: actionId,
                payload: nil
            )
            try requireExecution().publishChatEvent(
                roomId: roomId,
                peer: Self.hostUserPeer,
                event: .actionTriggered(trigger)
            )
        } catch is CancellationError {
            logger.debug("Rust chat runtime disposed before action \(actionId)")
        } catch {
            logger.error("Rust chat runtime failed to report action \(actionId): \(error)")
        }
    }

    func dispatchCommand(_ command: Chat.Command, roomId: String?) async -> Bool {
        do {
            try checkNotDisposed()
            guard let roomId else { throw ChatSeamError.roomlessChat }
            let chatCommand = ChatCommand(command: command.name, payload: command.arguments)
            try requireExecution().publishChatEvent(
                roomId: roomId,
                peer: Self.hostUserPeer,
                event: .command(chatCommand)
            )
            return true
        } catch {
            logger.error("Rust chat runtime failed to send a command: \(error)")
            return false
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

        // A release, not a close. The product's cards may still be asking for
        // the same worker, and the core stops it once the last request goes.
        releaseWorker()
        for (_, count) in openRooms {
            for _ in 0 ..< count {
                workers.releaseWorker(for: productId)
            }
        }
        openRooms = [:]

        logger.debug("Rust chat runtime disposed for: \(productId)")
    }
}

private extension TrUAPIChatHandler {
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

    /// The room screen does not wait for the product to take the event, and a lost event is logged.
    func publishRoomEvent(_ event: ChatEvent, roomId: String) {
        do {
            try requireExecution().publishChatEvent(roomId: roomId, peer: Self.hostUserPeer, event: event)
        } catch {
            logger.error("Rust chat runtime failed to send a room event to \(roomId): \(error)")
        }
    }

    /// The id under which the product knows a stored message. A message that the product pushed with
    /// `update_feed` has a store id that starts with its room.
    func productMessageId(_ storedId: Chat.MessageId, roomId: String) -> String {
        Chat.ProductMessageId.product(storedId, in: .chatExtension(productId, roomId: roomId))
    }

    /// A persisted message can decode before the product attaches. `ProductMessageDecoder`
    /// never evicts, so failing once breaks that cell for the session.
    func renderOutputs(
        _ request: ProductRendererRenderRequest,
        name: String
    ) async throws -> AsyncThrowingStream<ChatRendererOutput, Error> {
        let nodes = try await renderNodesWhenConnected(
            request,
            deadline: ContinuousClock.now + renderStartupWindow,
            name: name
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
    }

    func renderNodesWhenConnected(
        _ request: ProductRendererRenderRequest,
        deadline: ContinuousClock.Instant,
        name: String
    ) async throws -> AsyncThrowingStream<RendererNode, Error> {
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
                    logger.error("Custom render gave up waiting for the product: \(name)")
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

private extension ProductChatUserEvent {
    var chatEvent: ChatEvent {
        switch self {
        case let .messagePosted(messageId, text, replyTo):
            .messagePosted(messageId: messageId, body: .text(text: text), replyTo: replyTo)
        case let .attachmentPosted(messageId, body):
            .messagePosted(messageId: messageId, body: body.userWireBody ?? .text(text: ""), replyTo: nil)
        case let .edited(messageId, text):
            .userEdited(messageId: messageId, body: .text(text: text))
        case let .deleted(messageId):
            .userDeleted(messageId: messageId)
        case let .reacted(messageId, emoji, placed):
            .userReacted(messageId: messageId, emoji: emoji, placed: placed)
        case let .seen(messageId):
            .seen(messageId: messageId)
        }
    }
}
