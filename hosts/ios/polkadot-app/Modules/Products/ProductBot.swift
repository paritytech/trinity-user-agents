import Foundation
import PolkadotUI
import Products
import SubstrateSdk
import TrUAPIHost
import UIKitExt

/// A chat extension bot that delegates all behavior to a JavaScript product script.
///
/// The JS script controls:
/// - Welcome messages (via `onBotStarted`)
/// - Responses to user messages (via `onUserMessage`)
/// - Custom UI rendering (via `chatRenderWidget` / `chatSendCustomMessage`)
///
/// This bot just provides the bridge for JS to send messages to the chat.
/// Each instance is created by ``ProductBotFactory`` for a specific ``Product``.
final class ProductBot: ChatExtensionBot, @unchecked Sendable {
    let product: Product
    private let description: String?
    private let runtime: ChatRuntimeProtocol
    private let resolveImage: WidgetImageResolver
    private let rooms: ProductRoomReading
    private let logger: LoggerProtocol

    private var initTask: Task<Void, Never>?

    lazy var messageDecoder = ProductMessageDecoder(
        runtime: runtime,
        tokenResolver: WidgetDesignTokenResolver(),
        description: description,
        resolveImage: resolveImage,
        logger: logger
    )

    init(
        product: Product,
        description: String?,
        runtime: ChatRuntimeProtocol,
        resolveImage: WidgetImageResolver,
        rooms: ProductRoomReading = ProductRoomReader(storageFacade: UserDataStorageFacade.shared),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.product = product
        self.description = description
        self.runtime = runtime
        self.resolveImage = resolveImage
        self.rooms = rooms
        self.logger = logger
    }

    deinit {
        initTask?.cancel()
        // Enforce the runtime ownership contract even when the bot is dropped
        // without an explicit dispose() (e.g. the store discards it).
        Task { [runtime] in await runtime.dispose() }
    }

    // MARK: - ChatExtensionBot

    /// Each message that the user saves in a room is an action of the user: a post, a reply, an edit or a
    /// reaction. The product receives it as an event (02-DESIGN §5.4, P4.3).
    override func process(
        message: Chat.LocalMessage,
        lastProcessingOutcome: ChatExtension.ProcessingHistoryOutcome,
        context _: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        guard
            lastProcessingOutcome == .firstEncounter,
            message.origin == .user,
            let event = ProductChatUserEvent(userMessage: message) else {
            return .skipped
        }

        await publish(event, in: message.chatId)
        return .processed
    }

    /// `process(message:)` takes every user message, so the base class never asks for the text alone.
    override func onTextMessage(
        _ message: Chat.LocalMessage,
        text: String,
        context _: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        await publish(.messagePosted(messageId: message.messageId, text: text, replyTo: nil), in: message.chatId)
        return .processed
    }

    /// Tear down the JS engine and cancel background work.
    func dispose() async {
        initTask?.cancel()
        initTask = nil
        await runtime.dispose()
        logger.debug("Disposed product bot: \(product.name)")
    }
}

// MARK: - ChatCommandHandling

extension ProductBot: ChatCommandHandling {
    /// Only a command that the room declares goes to the product. Any other `/text` is a message
    /// (02-DESIGN §13 item 12).
    func handle(command: Chat.Command, roomId: String?) async -> Bool {
        guard
            let roomId,
            let commands = await rooms.declarations(of: chatId(roomId))?.commands,
            commands.contains(where: { $0.name == command.name }) else {
            return false
        }

        return await runtime.dispatchCommand(command, roomId: roomId)
    }
}

// MARK: - ChatUserDeletionHandling

extension ProductBot: ChatUserDeletionHandling {
    func userDeleted(messageId: Chat.MessageId, roomId: String?) async {
        let chatId = Chat.Id.chatExtension(identifier, roomId: roomId)
        await publish(.deleted(messageId: Chat.ProductMessageId.product(messageId, in: chatId)), in: chatId)
    }
}

// MARK: - ChatUserSeenHandling

extension ProductBot: ChatUserSeenHandling {
    /// The product gets the newest message that the user saw, when the room asks for it (`report_seen`).
    func userSaw(messageIds: Set<Chat.MessageId>, roomId: String?) async {
        guard let roomId else { return }

        let chatId = chatId(roomId)
        guard
            await rooms.declarations(of: chatId)?.effectiveConfig.reportSeen == true,
            let newest = await rooms.newestMessageId(among: messageIds, in: chatId) else {
            return
        }

        await publish(.seen(messageId: Chat.ProductMessageId.product(newest, in: chatId)), in: chatId)
    }
}

// MARK: - User events

private extension ProductBot {
    func chatId(_ roomId: String) -> Chat.Id {
        .chatExtension(identifier, roomId: roomId)
    }

    /// In a room whose product keeps the messages, these actions go to `user_action` and not to
    /// `action_subscribe` (02-DESIGN §13 item 11), so the bot sends no event for them.
    func publish(_ event: ProductChatUserEvent, in chatId: Chat.Id) async {
        if await rooms.declarations(of: chatId)?.effectiveConfig.productSource == true {
            logger.debug("No user event for a room whose product keeps the messages")
            return
        }

        do {
            try await runtime.publishUserEvent(event, roomId: chatId.roomId)
        } catch TrUAPIChatHandler.ChatSeamError.notStarted {
            // Driven before `start` opened the execution — an ordering issue, not
            // a product failure.
            logger.debug("User event not forwarded: the runtime has not started")
        } catch {
            logger.error("Failed to forward a user event to the product: \(error)")
        }
    }
}

// MARK: - Product rooms

extension ProductBot: ProductChatQuerying {
    func messagesPage(_ request: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryItem {
        try await productQuerying().messagesPage(request)
    }

    func userAction(_ request: ProductChatUserActionRequest) async throws -> ProductChatUserActionItem {
        try await productQuerying().userAction(request)
    }
}

extension ProductBot: ProductChatSearching {
    func searchPage(_ request: ProductChatSearchQueryRequest) async throws -> ProductChatSearchQueryItem {
        guard let searching = runtime as? ProductChatSearching else {
            throw ChatQueryError.refused(.notAvailable)
        }
        return try await searching.searchPage(request)
    }
}

extension ProductBot: ChatRoomLifecycle {
    /// A runtime without room lifecycle, the native one, holds its worker for the whole session.
    func open(roomId: String) async throws {
        try await (runtime as? ChatRoomLifecycle)?.open(roomId: roomId)
    }

    func close(roomId: String) async {
        await (runtime as? ChatRoomLifecycle)?.close(roomId: roomId)
    }
}

extension ProductBot: ProductRoomInteracting {
    func messageMenu(
        _ request: ProductChatMessageActionsQueryRequest
    ) async throws -> ProductChatMessageActionsQueryItem {
        try await roomInteracting.messageMenu(request)
    }

    func participants(_ request: ProductChatParticipantsQueryRequest) async throws -> ProductChatParticipantsQueryItem {
        try await roomInteracting.participants(request)
    }

    func participantMenu(
        _ request: ProductChatParticipantActionsQueryRequest
    ) async throws -> ProductChatParticipantActionsQueryItem {
        try await roomInteracting.participantMenu(request)
    }

    func reactions(_ request: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem {
        try await roomInteracting.reactions(request)
    }

    func publish(_ event: ChatEvent, roomId: String) async throws {
        try await roomInteracting.publish(event, roomId: roomId)
    }
}

extension ProductBot: ProductRoomFooterRendering {
    func renderFooter(roomId: String) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        guard let renderer = runtime as? ProductRoomFooterRendering else {
            return AsyncThrowingStream { $0.finish(throwing: UnservedProductRoom.NotServed()) }
        }
        return await renderer.renderFooter(roomId: roomId)
    }

    func dispatchFooterAction(roomId: String, actionId: String, payload: String?) async {
        await (runtime as? ProductRoomFooterRendering)?.dispatchFooterAction(
            roomId: roomId,
            actionId: actionId,
            payload: payload
        )
    }

    /// Loads the images of the trees that the product draws, with the cache of its bodies.
    var widgetImageResolver: WidgetImageResolver {
        resolveImage
    }
}

private extension ProductBot {
    var roomInteracting: ProductRoomInteracting {
        (runtime as? ProductRoomInteracting) ?? UnservedProductRoom()
    }

    /// The native runtime serves no product-source room, so its rooms show "unavailable".
    struct UnservedProductQuery: Error {}

    func productQuerying() throws -> ProductChatQuerying {
        guard let querying = runtime as? ProductChatQuerying else {
            throw UnservedProductQuery()
        }
        return querying
    }
}

// MARK: - ChatExtensionBotProtocol

extension ProductBot: ChatExtensionBotProtocol {
    var identifier: ChatExtension.Id {
        product.extensionId
    }

    var customDecoders: [ChatMessageCustomDecoding] {
        [messageDecoder]
    }

    var peerMetadata: Chat.PeerMetadata {
        Chat.PeerMetadata(
            name: product.name,
            icon: .product(domain: product.id),
            input: ChatRoomPolicy.defaults.peerInput,
            moreActions: ChatRoomPolicy.defaults.toolbar
        )
    }

    func deliverAutomaticMessages(_ context: ChatExtensionDiscoverContextProtocol) {
        initTask = Task { [weak self] in
            guard let self else { return }

            do {
                // The start holds no worker (02-DESIGN D10): the worker starts when a room screen opens or a
                // message wakes it. A product with no room gets one start wake (W2, 07-LATER §10.2).
                try await runtime.start(messagingSupport: .init(bot: self, context: context))

                #if targetEnvironment(simulator)
                    // The harness sends to the product while no room screen is open, so it holds the worker.
                    if TrUAPIE2ECommandFeed.holdsWorker(for: product.id),
                       let handler = runtime as? TrUAPIChatHandler {
                        try await handler.holdWorker()
                        await TrUAPIE2EWorkerHolds.register(
                            .init(
                                release: { await handler.releaseHeldWorker() },
                                hold: { try await handler.holdWorker() },
                                wake: { [workers = handler.workers, productId = product.id] reason in
                                    await workers.wake(productId, reason: reason)
                                }
                            ),
                            productId: product.id
                        )
                    }

                    // Drives the first user message for `make ios-chat-run`. The core
                    // buffers host actions until the product subscribes, so no wait.
                    let environment = ProcessInfo.processInfo.environment
                    if let message = environment["TRUAPI_IOS_E2E_CHAT_MESSAGE"],
                       let roomId = environment["TRUAPI_IOS_E2E_CHAT_ROOM_ID"] {
                        logger.debug("[e2e] posting trigger \(message) to room \(roomId)")
                        try await runtime.onUserMessage(text: message, roomId: roomId)
                    }
                #endif

                logger.debug("Initialized and started bot: \(product.name)")

                #if targetEnvironment(simulator)
                    // Sends the command list of `make ios-chat-v2-run`, one file at a time,
                    // until `dispose` stops this task.
                    if TrUAPIE2ECommandFeed.isEnabled(for: product.id) {
                        await TrUAPIE2ECommandFeed
                            .run(productId: product.id, logger: logger) { [runtime] text, roomId in
                                try await runtime.onUserMessage(text: text, roomId: roomId)
                            }
                    }
                #endif
            } catch is CancellationError {
                logger.debug("Start superseded by dispose for product bot: \(product.name)")
            } catch {
                logger.error("Failed to start product bot \(product.name): \(error)")
            }
        }
    }

    func process(action: Chat.Action, context: ChatExtensionActionContextProtocol) async {
        switch action {
        case let .customMessage(actionId, payload, messageId):
            // Passed as found: the native runtime addresses a body by message id
            // and answers without a room or a type.
            let message = await (try? context.getMessage(messageId: messageId))
            let messageType: String? =
                if case let .customRendered(content) = message?.content {
                    content.identifier
                } else {
                    nil
                }

            await runtime.dispatchEvent(
                roomId: message?.chatId.roomId,
                messageId: messageId,
                messageType: messageType,
                actionId: actionId,
                payload: payload as? String
            )
        case let .productButton(messageId, actionId):
            let message = await (try? context.getMessage(messageId: messageId))
            await runtime.dispatchActionTrigger(
                roomId: message?.chatId.roomId,
                messageId: messageId,
                actionId: actionId
            )
        }
    }

    func attach(presentationView view: ControllerBackedProtocol) {
        Task { @MainActor in
            runtime.attach(presentationView: view)
        }
    }
}
