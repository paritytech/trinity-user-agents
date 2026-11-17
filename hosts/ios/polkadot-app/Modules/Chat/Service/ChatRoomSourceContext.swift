import Foundation
import TrUAPIHost

/// What the room screen of a product room needs from its product.
struct ProductRoomServices: Sendable {
    /// The page and user-action queries. Nil when the product cannot answer them.
    let querying: ProductChatQuerying?
    let lifecycle: ChatRoomLifecycle

    init(querying: ProductChatQuerying?, lifecycle: ChatRoomLifecycle = NoChatRoomLifecycle()) {
        self.querying = querying
        self.lifecycle = lifecycle
    }

    /// The services that the bot of a product gives. A bot conforms to ``ProductChatQuerying`` and
    /// ``ChatRoomLifecycle`` to serve its rooms.
    init(bot: Any?) {
        self.init(
            querying: bot as? ProductChatQuerying,
            lifecycle: (bot as? ChatRoomLifecycle) ?? NoChatRoomLifecycle()
        )
    }
}

/// Picks the message source of a room, and keeps the open state of the room. Every room reads the host database.
actor ChatRoomSourceContext {
    private let chatId: Chat.Id
    private let coreDataSource: CoreDataMessageSource
    private let services: ProductRoomServices?
    private let logger: LoggerProtocol

    private var isOpen = false

    init(
        chatId: Chat.Id,
        coreDataSource: CoreDataMessageSource,
        services: ProductRoomServices?,
        logger: LoggerProtocol
    ) {
        self.chatId = chatId
        self.coreDataSource = coreDataSource
        self.services = services
        self.logger = logger
    }

    func source() async -> ChatMessageSourcing {
        coreDataSource
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
