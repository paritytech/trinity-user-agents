import Foundation
import SubstrateSdk

extension Chat {
    /// What a product declares for one of its rooms. The host keeps it in
    /// `CDChatRoomMetadata`, so the chat list can draw the room without the product worker.
    ///
    /// The store keeps the configuration as the SCALE bytes of its Chat Extension v2 wire value.
    /// A nil value means that the product declared nothing.
    struct RoomDeclarations: Equatable {
        let config: RoomConfig?
        /// Goes up by one at each configuration that the product sets, so an open room reloads its policy.
        let configGeneration: Int64
        let preview: RoomPreview

        /// The configuration that applies: the declared one, or the host default.
        var effectiveConfig: RoomConfig {
            config ?? .hostDefault
        }
    }

    struct RoomPreview: Equatable {
        /// The sort key in milliseconds since the Unix epoch. Nil uses the time of the last message.
        let timestamp: Int64?
        let badge: RoomBadge
        /// The last message of the chat list, as the room screen draws it. Nil draws the stored last message.
        let lastMessage: RoomPreviewMessage?
        /// The contact reacted to a message, and the user did not see the reaction yet.
        let unseenReaction: Bool

        /// The preview of a room whose product set none: the host draws the last message.
        static let hostDefault = RoomPreview(
            timestamp: nil,
            badge: .hostDefault,
            lastMessage: nil,
            unseenReaction: false
        )
    }

    /// The newest message of a room that the product keeps, which the chat list draws as the preview line.
    struct RoomPreviewMessage: Equatable {
        let messageId: MessageId
        let status: LocalMessage.Status
        let timestamp: Timestamp
        let content: LocalMessage.Content
        /// The edits of the message, which the native preview line reads.
        let relatedMessages: [RelatedLocalMessage]

        /// The last message of the chat list.
        func localMessage(chatId: Chat.Id) -> LocalMessage {
            LocalMessage(
                messageId: messageId,
                chatId: chatId,
                origin: status.isOutgoing ? .user : .chatExtension(chatId.extensionId ?? ""),
                creationSource: .localDevice,
                status: status,
                timestamp: timestamp,
                content: content,
                reactions: [],
                compactionId: nil,
                relatedMessages: relatedMessages
            )
        }
    }

    enum RoomBadge: Equatable {
        /// The host counts the unread messages.
        case hostDefault
        case none
        /// Zero shows no badge.
        case unreadCount(Int32)
    }

    /// The parts of a room that the product serves itself (`ChatRoomConfig`).
    struct RoomConfig: Equatable {
        /// The product keeps the messages, and the host keeps only a page cache.
        let productSource: Bool
        /// The host offers "who reacted" and asks the product for it.
        let reactionDetails: Bool
        /// The product wants a `Seen` event for the messages that the user saw.
        let reportSeen: Bool

        /// The configuration of a room that declares nothing.
        static let hostDefault = RoomConfig(productSource: false, reactionDetails: false, reportSeen: false)
    }
}

extension Chat {
    /// Replaces the name of a room and keeps its v1 icon.
    struct RoomNameUpdate {
        let chatId: Chat.Id
        let name: String
    }

    struct RoomConfigUpdate {
        let chatId: Chat.Id
        let config: RoomConfig
    }

    struct RoomPreviewUpdate {
        let chatId: Chat.Id
        let preview: RoomPreview
    }
}
