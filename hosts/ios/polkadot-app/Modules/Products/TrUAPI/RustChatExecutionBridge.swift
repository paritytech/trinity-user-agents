import Foundation
import FoundationExt
import Products
import TrUAPIHost

/// Per-execution bridge for the native Chat modality: a
/// ``RustProductExecutionBridge`` that also answers the rust core's
/// `ChatHostBridge` callbacks against the product's chat binding.
final class RustChatExecutionBridge: RustProductExecutionBridge, ChatHostBridge, @unchecked Sendable {
    private let chatMessaging: any ProductChatMessaging
    private let feedListeners: ProductFeedListeners
    private let pageCache: ProductMessageCaching
    // The base class keeps `dependencies` private; hold on to what this bridge reads here.
    private let productId: ProductId
    private let logger: LoggerProtocol

    init(
        dependencies: Dependencies,
        chatMessaging: any ProductChatMessaging,
        feedListeners: ProductFeedListeners = .shared,
        pageCache: ProductMessageCaching = ProductMessageCache.shared
    ) {
        self.chatMessaging = chatMessaging
        self.feedListeners = feedListeners
        self.pageCache = pageCache
        productId = dependencies.productId
        logger = dependencies.logger
        super.init(dependencies: dependencies)
    }

    func createRoom(roomId: String, name: String, icon: String) async throws -> ChatRoomRegistrationStatus {
        logger.debug("[truapi:chat-bridge] createRoom \(roomId)")
        // Same validation `postMessage` applies: an empty id names no room, and
        // the chat identifier built from it would be malformed.
        guard let roomId = roomId.nilIfEmpty else {
            throw HostRejection.Rejected(reason: "a chat room needs an id")
        }

        let result = try await chatMessaging.createRoom(CreateRoomRequest(
            roomId: roomId,
            name: name.nilIfEmpty,
            icon: icon.nilIfEmpty
        ))
        return switch result.status {
        case .new: .new
        case .exists: .exists
        }
    }

    func registerBot(botId: String, name _: String, icon _: String) async throws -> ChatBotRegistrationStatus {
        logger.debug("[truapi:chat-bridge] registerBot \(botId) -> rejecting")
        // No native bot registry; the container leaves it unimplemented too.
        throw HostRejection.Rejected(reason: "bot registration is not supported by this host")
    }

    func postMessage(roomId: String, content: ChatMessageContent, alt: String?) async throws -> String {
        // Message bodies are user content and this logger has a file destination
        // on testnet builds: log the variant, never the payload.
        logger.debug("[truapi:chat-bridge] postMessage \(roomId) \(content.variantName)")
        // This bridge validates product input whatever the core does upstream: a
        // roomless body lands in the bot's default chat, which no `RenderContext`
        // can name and so no renderer could ever draw.
        guard let roomId = roomId.nilIfEmpty else {
            throw HostRejection.Rejected(reason: "a chat message needs a room")
        }

        let message: ProductBotMessage =
            switch content {
            case let .text(text):
                .text(text)
            case let .custom(custom):
                .custom(messageType: custom.messageType, data: custom.payload, alt: alt)
            case .richText,
                 .actions,
                 .file,
                 .reaction,
                 .reactionRemoved:
                throw HostRejection.Rejected(reason: "this host renders text and custom messages only")
            }
        return try await chatMessaging.sendMessage(message, roomId: roomId)
    }

    func setRoomFooter(roomId: String, footer: ChatRoomFooter) async throws {
        logger.debug("[truapi:chat-bridge] setRoomFooter \(roomId)")
        guard let roomId = roomId.nilIfEmpty else {
            throw HostRejection.Rejected(reason: "a chat room needs an id")
        }

        let roomFooter: Chat.RoomFooter =
            switch footer {
            case .textInput: .textInput
            case .empty: .empty
            }
        try await chatMessaging.setRoomFooter(roomId: roomId, footer: roomFooter)
    }

    func listRooms() async throws -> [ChatRoom] {
        logger.debug("[truapi:chat-bridge] listRooms")
        // An empty result matches what the core publishes on failure anyway
        // (`list_rooms().unwrap_or_default()`).
        for try await rooms in try await chatMessaging.subscribeRooms() {
            return rooms.map { $0.toChatRoom() }
        }
        return []
    }

    // MARK: - Chat Extension v2

    // The core checked that the product hosts each room and screened every value. A room that is gone by now
    // answers `ChatRoomError.unknownRoom`, which the product receives as the reason of a rejection.

    func roomConfig(roomId: String) async throws -> ChatRoomConfig {
        try await chatMessaging.roomDeclarations(roomId: requireRoomId(roomId)).effectiveConfig.wireValue
    }

    func setRoomConfig(roomId: String, config: ChatRoomConfig) async throws {
        logger.debug("[truapi:chat-bridge] setRoomConfig \(roomId)")
        try await chatMessaging.setRoomConfig(roomId: requireRoomId(roomId), config: Chat.RoomConfig(config))
    }

    func setRoomMetadata(roomId: String, metadata: ChatRoomMetadata) async throws {
        logger.debug("[truapi:chat-bridge] setRoomMetadata \(roomId)")
        try await chatMessaging.setRoomMetadata(roomId: requireRoomId(roomId), name: metadata.name)
    }

    func setRoomPreview(roomId: String, preview: ChatRoomPreview) async throws {
        try await chatMessaging.setRoomPreview(roomId: requireRoomId(roomId), preview: Chat.RoomPreview(preview))
    }

    /// A delete of a room is idempotent: a room that is gone already is not an error.
    func deleteRoom(roomId: String) async throws {
        logger.debug("[truapi:chat-bridge] deleteRoom \(roomId)")
        do {
            try await chatMessaging.deleteRoom(roomId: requireRoomId(roomId))
        } catch ChatRoomError.unknownRoom {
            logger.debug("[truapi:chat-bridge] deleteRoom \(roomId): no such room")
        }
    }

    /// The core reads the configuration of the room and sets `persist` for a room that the host stores. For a
    /// room that the product keeps, an open room screen takes the update. When no screen shows the room, the update
    /// changes the cached page, so the next open shows the change before the newest page arrives (02-DESIGN §5.3).
    func updateFeed(roomId: String, update: ChatFeedUpdate, persist: Bool) async throws {
        guard let roomId = roomId.nilIfEmpty else {
            throw NativeChatFeedRejection.Rejected(reason: "a chat room needs an id")
        }

        let chatId = Chat.Id.chatExtension(productId, roomId: roomId)
        let localUpdate = Chat.ProductFeedUpdate(update)

        guard persist else {
            if await !feedListeners.deliver(update, roomId: roomId, to: chatId) {
                await pageCache.apply(update, productId: productId, roomId: roomId)
                logger.debug("[truapi:chat-bridge] updateFeed \(roomId): no open room, the cached page takes it")
            }
            return
        }

        do {
            try await chatMessaging.applyFeedUpdate(roomId: roomId, update: localUpdate)
        } catch let error as Chat.ProductFeedError {
            throw error.rejection
        } catch let ChatRoomError.unknownRoom(roomId) {
            throw NativeChatFeedRejection.Rejected(reason: "no chat room \(roomId)")
        }
    }

    // MARK: - Chat Extension v2 additions

    func capabilities() async throws -> [String] {
        ChatHostCapabilities.served
    }

    func roomFeatures(roomId: String) async throws -> ChatRoomFeatures {
        try await chatMessaging.roomFeatures(roomId: requireRoomId(roomId)).wireValue
    }

    /// The store keeps a feature that this build does not serve, so a later build applies it.
    func setRoomFeatures(roomId: String, features: ChatRoomFeatures) async throws {
        logger.debug("[truapi:chat-bridge] setRoomFeatures \(roomId)")
        try await chatMessaging.setRoomFeatures(roomId: requireRoomId(roomId), features: Chat.RoomFeatures(features))
    }

    /// The core calls it only when this host lists `call_buttons`. The call stays in memory for the room screen.
    func setRoomCall(roomId: String, call: ChatRoomCall?) async throws {
        logger.debug("[truapi:chat-bridge] setRoomCall \(roomId)")
        try await chatMessaging.setRoomCall(roomId: requireRoomId(roomId), call: call.map(Chat.RoomCall.init))
    }

    /// The core calls it only when this host lists `contact_rooms`. The contact stays across restarts.
    func setRoomContact(roomId: String, contact: ChatRoomContact) async throws {
        logger.debug("[truapi:chat-bridge] setRoomContact \(roomId)")
        try await chatMessaging.setRoomContact(roomId: requireRoomId(roomId), contact: Chat.RoomContact(contact))
    }
}

private extension RustChatExecutionBridge {
    /// The core never sends an empty room id. The check keeps a broken call away from the chat store.
    func requireRoomId(_ roomId: String) throws -> String {
        guard let roomId = roomId.nilIfEmpty else {
            throw HostRejection.Rejected(reason: "a chat room needs an id")
        }
        return roomId
    }
}

extension RoomInfo {
    func toChatRoom() -> ChatRoom {
        let participatingAs: ChatRoomParticipation =
            switch participation {
            case .roomHost: .roomHost
            case .bot: .bot
            }
        return ChatRoom(roomId: roomId, participatingAs: participatingAs)
    }
}

private extension ChatMessageContent {
    var variantName: String {
        switch self {
        case .text: "text"
        case .richText: "richText"
        case .custom: "custom"
        case .actions: "actions"
        case .file: "file"
        case .reaction: "reaction"
        case .reactionRemoved: "reactionRemoved"
        }
    }
}
