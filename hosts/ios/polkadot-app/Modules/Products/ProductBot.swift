import Foundation
import Products
import SubstrateSdk
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
final class ProductBot: ChatExtensionBot {
    let product: Product
    private let runtime: ChatRuntimeProtocol
    private let logger: LoggerProtocol

    private var initTask: Task<Void, Never>?

    lazy var messageDecoder = ProductMessageDecoder(
        runtime: runtime,
        tokenResolver: WidgetDesignTokenResolver(),
        logger: logger
    )

    init(
        product: Product,
        runtime: ChatRuntimeProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.product = product
        self.runtime = runtime
        self.logger = logger
    }

    deinit {
        initTask?.cancel()
        // Enforce the runtime ownership contract even when the bot is dropped
        // without an explicit dispose() (e.g. the store discards it).
        Task { [runtime] in await runtime.dispose() }
    }

    // MARK: - ChatExtensionBot

    override func onTextMessage(
        _ message: Chat.LocalMessage,
        text: String,
        context _: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        do {
            try await runtime.onUserMessage(text: text, roomId: message.chatId.roomId)
        } catch TrUAPIChatHandler.ChatSeamError.notStarted {
            // Driven before `start` opened the execution — an ordering issue, not
            // a product failure.
            logger.debug("User message not forwarded: the runtime has not started")
        } catch {
            logger.error("Failed to forward user message to script: \(error)")
        }

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
            contactSource: .chat,
            icon: .image(nil),
            input: .inputField(.init(canPay: false, canAttachFile: false)),
            moreActions: []
        )
    }

    func deliverAutomaticMessages(_ context: ChatExtensionDiscoverContextProtocol) {
        initTask = Task { [weak self] in
            guard let self else { return }

            do {
                try await runtime.start(messagingSupport: .init(bot: self, context: context))

                #if targetEnvironment(simulator)
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
        }
    }

    func attach(presentationView view: ControllerBackedProtocol) {
        Task { @MainActor in
            runtime.attach(presentationView: view)
        }
    }
}
