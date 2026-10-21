import Foundation

extension Chat {
    /// What a product declares for one of its rooms (02-DESIGN §8.2). The host keeps it in
    /// `CDChatRoomMetadata`, so the chat list can draw the room without the product worker.
    ///
    /// The store keeps the configuration and the footer as the SCALE bytes of the Chat Extension v2 wire values.
    /// A nil value means that the product declared nothing.
    struct RoomDeclarations: Equatable {
        let description: String?
        /// Nil draws the icon of the product.
        let avatar: RoomImage?
        let config: RoomConfig?
        /// Goes up by one at each configuration that the product sets, so an open room reloads its policy.
        let configGeneration: Int64
        let preview: RoomPreview
        let footerConfig: RoomFooterConfig?
        /// The optional features of the Chat Extension v2 additions. A feature that this build does not serve
        /// stays here, and the host ignores it.
        var features: RoomFeatures = .hostDefault
        /// The mute and the archive state that the user set. The product does not declare it, but the host
        /// keeps it in the same row, so the chat list reads it with the declarations.
        var userState: RoomUserState = .default

        /// The configuration that applies: the declared one, or the host default.
        var effectiveConfig: RoomConfig {
            config ?? .hostDefault
        }
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

    /// The parts of a room that the product serves itself (`ChatRoomConfig`).
    struct RoomConfig: Equatable {
        /// The product keeps the messages, and the host keeps only a page cache.
        let productSource: Bool
        /// The host offers "who reacted" and asks the product for it.
        let reactionDetails: Bool
        /// The host asks the product for the participants.
        let participants: Bool
        /// The host asks the product for the menu of a message.
        let messageActions: Bool
        /// The emoji of the reaction picker. Nil offers all emoji.
        let allowedReactions: [String]?
        /// The text of a room without messages. Nil shows the host text.
        let placeholder: String?
        /// The product wants a `Seen` event for the messages that the user saw.
        let reportSeen: Bool

        /// The configuration of a room that declares nothing.
        static let hostDefault = RoomConfig(
            productSource: false,
            reactionDetails: false,
            participants: false,
            messageActions: false,
            allowedReactions: nil,
            placeholder: nil,
            reportSeen: false
        )
    }

    /// Bytes that a product gives for an icon or a media item (`ChatResource`).
    enum ProductResource: Equatable {
        case inline(mimeType: String, data: Data)
        case preimage(key: Data)
    }

    /// What the host does when the user selects a product action (`ChatActionHandler`).
    enum ProductActionHandler: Equatable {
        /// The host opens the link.
        case deeplink(url: String)
        /// The host sends the event of the action to the product.
        case callback
    }

    /// A menu entry that a product defines (`ChatProductAction`).
    struct ProductMenuAction: Equatable, Sendable {
        let actionId: String
        let label: String
        let icon: ProductResource?
        /// The menu draws the entry in red, after the other entries.
        let destructive: Bool
        let handler: ProductActionHandler
    }

    /// The payment button of the input (`ChatInputPayment`).
    enum RoomInputPayment: Equatable {
        /// The 32 bytes of the account that gets the payment.
        case enabled(destination: Data)
    }

    /// The input below the messages (`ChatInputConfig`).
    struct RoomInputConfig: Equatable {
        /// The hint text of the empty input. Nil shows the host text.
        let placeholder: String?
        let photo: Bool
        let video: Bool
        let file: Bool
        let payment: RoomInputPayment?

        /// The input of a room that declares nothing: photo and video on, file and payment off.
        static let hostDefault = RoomInputConfig(placeholder: nil, photo: true, video: true, file: false, payment: nil)
    }

    /// What the room shows below its messages (`ChatRoomFooter`, second version).
    enum RoomFooterConfig: Equatable {
        case input(RoomInputConfig)
        case empty
        /// The product draws the footer.
        case custom
    }

}

extension Chat {
    struct RoomDescriptionUpdate {
        let chatId: Chat.Id
        let description: String?
        let avatar: RoomImage?
    }

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

    /// Replaces the footer of a room.
    struct RoomFooterConfigUpdate {
        let chatId: Chat.Id
        let footer: RoomFooterConfig
    }

    /// The declarations of a stored room, as the read mapper gives them.
    struct StoredRoomDeclarations {
        let chatId: String
        let declarations: RoomDeclarations
    }
}
