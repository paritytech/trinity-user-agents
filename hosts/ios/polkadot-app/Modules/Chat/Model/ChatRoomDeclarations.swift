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
        /// The text that the room screen shows while the room has no messages. Nil shows nothing.
        let placeholder: String?

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
        /// The chat list shows the room above the rooms that are not pinned.
        let pinned: Bool
        /// A line that replaces the preview line. Nil draws the preview line of the last message.
        let activity: RoomActivity?

        /// The preview of a room whose product set none: the host draws the last message.
        static let hostDefault = RoomPreview(
            timestamp: nil,
            badge: .hostDefault,
            lastMessage: nil,
            unseenReaction: false,
            pinned: false,
            activity: nil
        )
    }

    /// A line that the chat list shows for a room in place of its last message, for example "Ben is typing".
    struct RoomActivity: Equatable {
        let text: String
        /// The activity continues, so the chat list animates the line.
        let ongoing: Bool
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
        /// An image in place of the unread count: an `https` URL or an inline `data:` image.
        case image(String)
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
    /// What `set_room_metadata` does to the icon of a room.
    enum RoomIconChange: Equatable {
        /// The room keeps its current icon.
        case keep
        /// The room shows this icon. Nil removes the icon.
        case replace(String?)
    }

    /// Replaces the name and the placeholder of a room, and changes its icon as `icon` tells.
    struct RoomNameUpdate {
        let chatId: Chat.Id
        let name: String
        let icon: RoomIconChange
        let placeholder: String?
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
