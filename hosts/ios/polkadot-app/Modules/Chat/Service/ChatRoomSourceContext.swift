import Foundation
import TrUAPIHost

/// What the room screen of a product room needs from its product (P5.2, P5.7).
struct ProductRoomServices: Sendable {
    /// The page and user-action queries. Nil when the product cannot answer them, so a product-source room
    /// shows "unavailable".
    let querying: ProductChatQuerying?
    let lifecycle: ChatRoomLifecycle
    let feedListeners: ProductFeedListeners
    let cache: ProductMessageCaching
    let settings: ProductMessageSource.Settings
    /// The outbox of the staged attachments (P10.3). Nil shows no outbox entry in the window.
    let stager: ProductAttachmentStager?
    /// The record of the `Preparing` messages, for the reasoning rows (07-LATER §9.4). Nil records nothing.
    let preparingMessages: ChatPreparingMessageTracking?

    init(
        querying: ProductChatQuerying?,
        lifecycle: ChatRoomLifecycle = NoChatRoomLifecycle(),
        feedListeners: ProductFeedListeners = .shared,
        cache: ProductMessageCaching = ProductMessageCache.shared,
        settings: ProductMessageSource.Settings = .init(),
        stager: ProductAttachmentStager? = nil,
        preparingMessages: ChatPreparingMessageTracking? = nil
    ) {
        self.querying = querying
        self.lifecycle = lifecycle
        self.feedListeners = feedListeners
        self.cache = cache
        self.settings = settings
        self.stager = stager
        self.preparingMessages = preparingMessages
    }

    /// The services that the bot of a product gives. A bot conforms to ``ProductChatQuerying`` and
    /// ``ChatRoomLifecycle`` to serve product-source rooms.
    init(bot: Any?) {
        self.init(
            querying: bot as? ProductChatQuerying,
            lifecycle: (bot as? ChatRoomLifecycle) ?? NoChatRoomLifecycle(),
            stager: .shared,
            preparingMessages: ChatPreparingMessageStore.shared
        )
    }
}

/// Picks the message source of a room from its policy, once, and keeps the open state of the room.
actor ChatRoomSourceContext {
    private let chatId: Chat.Id
    private let coreDataSource: CoreDataMessageSource
    private let services: ProductRoomServices?
    private let policy: @Sendable () async -> ChatRoomPolicy?
    private let logger: LoggerProtocol

    private var resolution: Task<ChatMessageSourcing, Never>?
    private var productSource: ProductMessageSource?
    private var registration: ProductFeedListeners.Registration?
    private var isOpen = false

    init(
        chatId: Chat.Id,
        coreDataSource: CoreDataMessageSource,
        services: ProductRoomServices?,
        policy: @escaping @Sendable () async -> ChatRoomPolicy?,
        logger: LoggerProtocol
    ) {
        self.chatId = chatId
        self.coreDataSource = coreDataSource
        self.services = services
        self.policy = policy
        self.logger = logger
    }

    func source() async -> ChatMessageSourcing {
        if let resolution {
            return await resolution.value
        }

        let task = Task { await self.resolve() }
        resolution = task
        return await task.value
    }

    func messageStore() async -> ChatRoomPolicy.MessageStore {
        await source() is ProductMessageSource ? .product : .host
    }

    /// Holds the product of the room while the room screen shows it.
    func open() async {
        guard !isOpen, let services, let roomId = chatId.roomId else {
            return
        }

        do {
            try await services.lifecycle.open(roomId: roomId)
            isOpen = true
        } catch {
            logger.warning("Room \(roomId) did not open: \(error)")
            _ = await source()
            await productSource?.markUnavailable()
        }
    }

    func close() async {
        guard isOpen, let services, let roomId = chatId.roomId else {
            return
        }

        isOpen = false
        await services.lifecycle.close(roomId: roomId)
    }

    /// Opens the room again when the last open failed, then asks for the newest page.
    func retry() async {
        await open()
        await source().reload()
    }
}

private extension ChatRoomSourceContext {
    func resolve() async -> ChatMessageSourcing {
        guard
            let services,
            case let .chatExtension(productId, roomId?) = chatId,
            let policy = await policy(),
            policy.messageStore == .product
        else {
            return coreDataSource
        }

        var settings = services.settings
        settings.reportsSeen = policy.reportsSeen

        let source = ProductMessageSource(
            chatId: chatId,
            productId: productId,
            roomId: roomId,
            querying: services.querying ?? MissingProductChatQuerying(),
            cache: services.cache,
            settings: settings,
            stager: services.stager,
            preparingMessages: services.preparingMessages,
            logger: logger
        )
        registration = services.feedListeners.register(source, for: chatId)
        productSource = source
        return source
    }
}

/// The queries of a product that does not serve product-source rooms. Each query fails as unavailable.
private struct MissingProductChatQuerying: ProductChatQuerying {
    struct NotServed: Error {}

    func messagesPage(_: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryItem {
        throw NotServed()
    }

    func userAction(_: ProductChatUserActionRequest) async throws -> ProductChatUserActionItem {
        throw NotServed()
    }
}
