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
    /// The outbox of the messages of the user (P10.3). Nil sends through an outbox in memory of the room.
    let outbox: ProductOutbox?
    /// Takes the photos and the videos of the contact. Nil takes none.
    let downloads: ProductMediaDownloads?
    /// The payments that the host keeps for the rooms. Nil shows the payments as the product gives them.
    let payments: ProductChatPayments?

    init(
        querying: ProductChatQuerying?,
        lifecycle: ChatRoomLifecycle = NoChatRoomLifecycle(),
        feedListeners: ProductFeedListeners = .shared,
        cache: ProductMessageCaching = ProductMessageCache.shared,
        settings: ProductMessageSource.Settings = .init(),
        outbox: ProductOutbox? = nil,
        downloads: ProductMediaDownloads? = nil,
        payments: ProductChatPayments? = nil
    ) {
        self.querying = querying
        self.lifecycle = lifecycle
        self.feedListeners = feedListeners
        self.cache = cache
        self.settings = settings
        self.outbox = outbox
        self.downloads = downloads
        self.payments = payments
    }

    /// The services that the bot of a product gives. A bot conforms to ``ProductChatQuerying`` and
    /// ``ChatRoomLifecycle`` to serve product-source rooms.
    init(bot: Any?) {
        self.init(
            querying: bot as? ProductChatQuerying,
            lifecycle: (bot as? ChatRoomLifecycle) ?? NoChatRoomLifecycle(),
            outbox: .shared,
            downloads: .shared,
            payments: .shared
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

    /// Picks the source again when the policy of the room now names the other message store: the product set the
    /// configuration of the room after the screen picked a source. Gives true when the source changed.
    func refresh() async -> Bool {
        guard resolution != nil else {
            return false
        }

        let resolvedProduct = await source() is ProductMessageSource
        guard await wantsProductSource() != resolvedProduct else {
            return false
        }

        logger.debug("Room \(chatId.rawRepresentation) changed its message store")
        registration?.cancel()
        registration = nil
        productSource = nil
        resolution = nil
        _ = await source()
        return true
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
}

private extension ChatRoomSourceContext {
    func wantsProductSource() async -> Bool {
        guard services != nil, chatId.roomId != nil else {
            return false
        }

        return await policy()?.messageStore == .product
    }

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
            outbox: services.outbox,
            downloads: services.downloads,
            payments: services.payments,
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
