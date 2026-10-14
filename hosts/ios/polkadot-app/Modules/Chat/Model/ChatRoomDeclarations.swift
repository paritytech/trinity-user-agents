import Foundation

extension Chat {
    /// What a product declares for one of its rooms (02-DESIGN §8.2). The host keeps it in
    /// `CDChatRoomMetadata`, so the chat list can draw the room without the product worker.
    ///
    /// The configuration, toolbar, suggestions, commands and footer are the SCALE bytes of the Chat
    /// Extension v2 values. They stay opaque here until the Swift bindings of those values exist.
    struct RoomDeclarations: Equatable {
        let description: String?
        /// Nil draws the icon of the product.
        let avatar: RoomImage?
        let config: Data?
        /// Goes up by one at each configuration that the product sets, so an open room reloads its policy.
        let configGeneration: Int64
        let preview: RoomPreview
        let toolbar: Data?
        let suggestions: Data?
        let commands: Data?
        let footerConfig: Data?
    }

    /// An image that a product gives to a room, for its avatar or its badge.
    enum RoomImage: Equatable {
        /// The bytes of the image. The core limits their size.
        case inline(Data)
        /// The key of a preimage that the resource resolver gets.
        case preimage(key: Data)
    }

    struct RoomPreview: Equatable {
        enum Order: Int16 {
            case byTimestamp = 0
            case pinToTop = 1
        }

        let order: Order
        /// The sort key in milliseconds since the Unix epoch. Nil uses the time of the last message.
        let timestamp: Int64?
        let badge: RoomBadge
        /// Nil shows the last message.
        let text: String?
        /// The text shows as an activity that continues, for example "Alice is typing".
        let ongoing: Bool

        /// The preview of a room whose product set none: the host draws the last message.
        static let hostDefault = RoomPreview(
            order: .byTimestamp,
            timestamp: nil,
            badge: .hostDefault,
            text: nil,
            ongoing: false
        )
    }

    enum RoomBadge: Equatable {
        /// The host counts the unread messages.
        case hostDefault
        case none
        /// Zero shows no badge.
        case unreadCount(Int32)
        case image(RoomImage)
    }
}

extension Chat {
    struct RoomDescriptionUpdate {
        let chatId: Chat.Id
        let description: String?
        let avatar: RoomImage?
    }

    struct RoomConfigUpdate {
        let chatId: Chat.Id
        let config: Data
    }

    struct RoomPreviewUpdate {
        let chatId: Chat.Id
        let preview: RoomPreview
    }

    /// Replaces one list that the product declares for the room, or clears it with nil.
    struct RoomListUpdate {
        enum Kind {
            case toolbar
            case suggestions
            case commands
            case footer
        }

        let chatId: Chat.Id
        let kind: Kind
        let value: Data?
    }

    /// The declarations of a stored room, as the read mapper gives them.
    struct StoredRoomDeclarations {
        let chatId: String
        let declarations: RoomDeclarations
    }
}
