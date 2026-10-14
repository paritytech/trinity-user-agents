import Foundation
import FoundationExt
import Products
import TrUAPIHost

/// Per-execution bridge for the native Chat modality: a
/// ``RustProductExecutionBridge`` that also answers the rust core's
/// `ChatHostBridge` callbacks against the product's chat binding.
final class RustChatExecutionBridge: RustProductExecutionBridge, ChatHostBridge, @unchecked Sendable {
    private let chatMessaging: any ProductChatMessaging
    // The base class keeps `dependencies` private; hold on to the logger here.
    private let logger: LoggerProtocol

    init(dependencies: Dependencies, chatMessaging: any ProductChatMessaging) {
        self.chatMessaging = chatMessaging
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

        return try await chatMessaging.sendMessage(content.toBotMessage(alt: alt), roomId: roomId)
    }

    func setRoomFooter(roomId: String, footer: ChatRoomFooter) async throws {
        logger.debug("[truapi:chat-bridge] setRoomFooter \(roomId)")
        guard let roomId = roomId.nilIfEmpty else {
            throw HostRejection.Rejected(reason: "a chat room needs an id")
        }

        // A footer that the product draws waits for P6.6.
        if case .custom = footer {
            throw HostRejection.Rejected(reason: "this host does not draw a custom chat footer")
        }
        try await chatMessaging.setRoomFooterConfig(roomId: roomId, footer: Chat.RoomFooterConfig(footer))
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
        try await chatMessaging.setRoomMetadata(
            roomId: requireRoomId(roomId),
            name: metadata.name,
            description: metadata.description,
            avatar: metadata.avatarImage
        )
    }

    func setRoomPreview(roomId: String, preview: ChatRoomPreview) async throws {
        try await chatMessaging.setRoomPreview(roomId: requireRoomId(roomId), preview: Chat.RoomPreview(preview))
    }

    /// Deleting a room is idempotent: a room that is gone already is not an error.
    func deleteRoom(roomId: String) async throws {
        logger.debug("[truapi:chat-bridge] deleteRoom \(roomId)")
        do {
            try await chatMessaging.deleteRoom(roomId: requireRoomId(roomId))
        } catch ChatRoomError.unknownRoom {
            logger.debug("[truapi:chat-bridge] deleteRoom \(roomId): no such room")
        }
    }

    func setRoomToolbar(roomId: String, actions: [ChatToolbarAction]) async throws {
        try await chatMessaging.setRoomToolbar(
            roomId: requireRoomId(roomId),
            actions: actions.map(Chat.RoomToolbarAction.init)
        )
    }

    func setRoomSuggestions(roomId: String, suggestions: [ChatSuggestion]) async throws {
        try await chatMessaging.setRoomSuggestions(
            roomId: requireRoomId(roomId),
            suggestions: suggestions.map(Chat.RoomSuggestion.init)
        )
    }

    func setRoomCommands(roomId: String, commands: [ChatCommandSpec]) async throws {
        try await chatMessaging.setRoomCommands(
            roomId: requireRoomId(roomId),
            commands: commands.map(Chat.RoomCommand.init)
        )
    }

    func setRoomActivity(roomId: String, activity: ChatActivity?) async throws {
        try await chatMessaging.setRoomActivity(
            roomId: requireRoomId(roomId),
            activity: activity.map(Chat.RoomActivity.init)
        )
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
    /// The core screened every field: media and file URLs are `https` or inline images.
    func toBotMessage(alt: String?) throws -> ProductBotMessage {
        switch self {
        case let .text(text):
            .text(text)
        case let .custom(custom):
            .custom(messageType: custom.messageType, data: custom.payload, alt: alt)
        case let .richText(richText):
            .richText(text: richText.text, media: richText.media.map(\.url))
        case let .actions(actions):
            .actions(ProductBotActions(
                text: actions.text,
                buttons: actions.actions.map { .init(actionId: $0.actionId, title: $0.title) },
                layout: actions.layout == .grid ? .grid : .column
            ))
        case let .file(file):
            .file(ProductBotFile(
                url: file.url,
                fileName: file.fileName,
                mimeType: file.mimeType,
                sizeBytes: file.sizeBytes,
                text: file.text
            ))
        case .reaction,
             .reactionRemoved:
            throw HostRejection.Rejected(reason: "this host does not apply reactions from a product")
        }
    }

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
