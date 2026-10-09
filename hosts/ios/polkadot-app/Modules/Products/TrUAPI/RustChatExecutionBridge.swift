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

    func postMessage(roomId: String, content: ChatMessageContent) async throws -> String {
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
                .custom(messageType: custom.messageType, data: custom.payload)
            case .richText,
                 .actions,
                 .file,
                 .reaction,
                 .reactionRemoved:
                throw HostRejection.Rejected(reason: "this host renders text and custom messages only")
            }
        return try await chatMessaging.sendMessage(message, roomId: roomId)
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
