import Foundation
import Products
import StructuredConcurrency
import TrUAPIHost
import UIKitExt

/// The chat half of a product's worker.
///
/// A Chat session claims its product's worker until the session closes.
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

    private let productId: ProductId
    private let runtime: ProductWorkerRuntimeProvider
    /// The product's own chat surface and routers, which outlive any one worker.
    private let context: ProductWorkerContext
    private let logger: LoggerProtocol

    /// What this session holds of the product's worker request, so dispose
    /// gives back exactly what start asked for and never more.
    private var workerClaim: NativeWorkerClaim?
    private var workerFollowingTask: Task<Void, Never>?
    private var roomsForwardingTask: Task<Void, Never>?
    /// The native room list, told to each execution the worker runs on.
    private var rooms: [ChatRoom]?
    /// Renders still waiting for the worker or drawing, ended by dispose.
    private var renders: [UUID: Task<Void, Never>] = [:]
    private var started = false
    private var disposed = false

    init(
        productId: ProductId,
        runtime: @escaping ProductWorkerRuntimeProvider,
        context: ProductWorkerContext,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productId = productId
        self.runtime = runtime
        self.context = context
        self.logger = logger
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
        try await requireExecution().publishChatAction(action: HostChatActionSubscribeItem(
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
        guard let roomId else {
            return AsyncThrowingStream { $0.finish(throwing: ChatSeamError.roomlessChat) }
        }
        return render(
            .chatMessage(roomId: roomId, messageId: messageId, messageType: messageType),
            payload: messageData
        )
    }

    /// Draws the card the product docked in the footer of `roomId`.
    func renderRoomFooter(
        roomId: String,
        messageType: String,
        payload: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        render(.chatRoomFooter(roomId: roomId, messageType: messageType), payload: payload)
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
            try await publishRendererAction(
                .chatMessage(roomId: roomId, messageId: messageId, messageType: messageType),
                actionId: actionId,
                payload: payload
            )
            logger.debug("Rust chat runtime dispatched event \(actionId) for message \(messageId) to \(productId)")
        } catch is CancellationError {
            logger.debug("Rust chat runtime disposed before event \(actionId)")
        } catch {
            logger.error("Rust chat runtime failed to dispatch event \(actionId): \(error)")
        }
    }

    /// A press inside the card the product docked in the footer of `roomId`.
    func dispatchRoomFooterEvent(
        roomId: String,
        messageType: String,
        actionId: String,
        payload: String?
    ) async {
        do {
            try checkNotDisposed()
            try await publishRendererAction(
                .chatRoomFooter(roomId: roomId, messageType: messageType),
                actionId: actionId,
                payload: payload
            )
            logger.debug("Rust chat runtime dispatched footer event \(actionId) in room \(roomId) to \(productId)")
        } catch is CancellationError {
            logger.debug("Rust chat runtime disposed before footer event \(actionId)")
        } catch {
            logger.error("Rust chat runtime failed to dispatch footer event \(actionId): \(error)")
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

        workerFollowingTask?.cancel()
        workerFollowingTask = nil
        roomsForwardingTask?.cancel()
        roomsForwardingTask = nil
        for render in renders.values {
            render.cancel()
        }
        renders = [:]

        // The core keeps the bridge, and the bridge keeps the surface: unbinding
        // is what releases the chat context. Only this runtime's own binding,
        // because the surface is the product's and another runtime may hold it.
        context.chat.unbind(owner: self)

        // A release, not a close. The product's cards may still be asking for
        // the same worker, and the core stops it once the last request goes.
        workerClaim?.release()
        workerClaim = nil

        logger.debug("Rust chat runtime disposed for: \(productId)")
    }
}

private extension TrUAPIChatHandler {
    /// Asks for the worker and returns at once. The core starts it, retries it
    /// and gives up on a start that hangs, so there is nothing to wait for here.
    func startRuntime(
        messagingSupport: ProductsNativeApi.MessagingSupport
    ) async throws {
        // Bound before the worker is asked for, so the core can never reach a
        // surface with no binding.
        context.chat.bind(messagingSupport, owner: self)

        let host = try runtime()
        workerClaim = host.claimWorker(productId: productId)
        startRoomsForwarding(host)

        logger.debug("Rust chat runtime started for: \(productId)")
    }

    func checkNotDisposed() throws {
        guard !disposed else { throw CancellationError() }
    }

    /// A persisted message can decode before the product's worker runs, and
    /// `ProductMessageDecoder` never evicts, so the render waits for the worker
    /// instead of failing the cell. Dispose ends the wait.
    func render(
        _ context: RenderContext,
        payload: Data
    ) -> AsyncThrowingStream<ChatRendererOutput, Error> {
        let request = ProductRendererRenderRequest(context: context, payload: payload)
        let id = UUID()
        return AsyncThrowingStream { continuation in
            let task = Task { [weak self] in
                do {
                    guard let execution = try await self?.executionWhenRunning() else { throw CancellationError() }
                    for try await node in try execution.render(request) {
                        continuation.yield(.native(node))
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
                await self?.renderEnded(id)
            }
            renders[id] = task
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    func renderEnded(_ id: UUID) {
        renders[id] = nil
    }

    func executionWhenRunning() async throws -> NativeWorkerExecution? {
        try checkNotDisposed()
        return try await runtime().executionWhenRunning(productId)
    }

    func publishRendererAction(_ context: RenderContext, actionId: String, payload: String?) async throws {
        try await requireExecution().publishRendererAction(item: HostRendererActionSubscribeItem(
            context: context,
            actionId: actionId,
            payload: payload.map { Data($0.utf8) } ?? Data()
        ))
    }

    func requireExecution() async throws -> NativeWorkerExecution {
        guard let execution = await (try? runtime())?.runningExecution(productId) else {
            throw ChatSeamError.notStarted
        }
        return execution
    }

    /// Mirror the native room list into the core so product-side
    /// `chat.listSubscribe` sees native changes as they happen. Each execution
    /// the worker runs on is told the list too, so one that starts after the
    /// list was read, or restarts under this session, does not miss it.
    func startRoomsForwarding(_ host: any ProductWorkerRuntime) {
        roomsForwardingTask = Task { [weak self, logger, chat = context.chat] in
            do {
                for try await rooms in try await chat.subscribeRooms() {
                    guard !Task.isCancelled else { return }
                    await self?.roomsChanged(to: rooms.map { $0.toChatRoom() }, on: host)
                }
            } catch {
                guard !Task.isCancelled else { return }
                logger.error("Rust chat runtime rooms forwarding ended: \(error)")
            }
        }
        workerFollowingTask = Task { [weak self, productId] in
            await host.withRunningWorker(productId) { execution in
                guard let rooms = await self?.rooms else { return }
                execution.notifyChatRoomsChanged(rooms: rooms)
            }
        }
    }

    func roomsChanged(to rooms: [ChatRoom], on host: any ProductWorkerRuntime) async {
        self.rooms = rooms
        await host.runningExecution(productId)?.notifyChatRoomsChanged(rooms: rooms)
    }
}
