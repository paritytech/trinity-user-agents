import Foundation

extension Chat {
    /// What a product declares for one of its rooms (02-DESIGN §8.2). The host keeps it in
    /// `CDChatRoomMetadata`, so the chat list can draw the room without the product worker.
    ///
    /// The store keeps the configuration, the toolbar, the suggestions, the commands and the footer as the
    /// SCALE bytes of the Chat Extension v2 wire values. A nil value means that the product declared nothing.
    struct RoomDeclarations: Equatable {
        let description: String?
        /// Nil draws the icon of the product.
        let avatar: RoomImage?
        let config: RoomConfig?
        /// Goes up by one at each configuration that the product sets, so an open room reloads its policy.
        let configGeneration: Int64
        let preview: RoomPreview
        let toolbar: [RoomToolbarAction]?
        let suggestions: [RoomSuggestion]?
        let commands: [RoomCommand]?
        let footerConfig: RoomFooterConfig?

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
        /// The host asks the product for the menu of a participant.
        let participantActions: Bool
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
            participantActions: false,
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
        /// The host sends the matching event to the product.
        case callback
    }

    /// A button of the room toolbar (`ChatToolbarAction`).
    struct RoomToolbarAction: Equatable {
        let actionId: String
        let label: String
        let icon: ProductResource
        let handler: ProductActionHandler
    }

    /// A quick reply above the input (`ChatSuggestion`).
    struct RoomSuggestion: Equatable {
        let suggestionId: String
        let text: String
        /// A press sends the text as a message of the user. Else a press sends only an event.
        let sendsText: Bool
    }

    /// A slash command that the room declares (`ChatCommandSpec`).
    struct RoomCommand: Equatable {
        /// The name without the slash.
        let name: String
        let description: String
        let argumentHint: String?
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

    /// The activity line of a room, for example "Bot is typing" (`ChatActivity`).
    struct RoomActivity: Equatable, Sendable {
        let text: String
        /// The participant that the line is about. Nil is the product.
        let participantId: String?
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

    /// Replaces the toolbar buttons of a room. An empty list removes the toolbar.
    struct RoomToolbarUpdate {
        let chatId: Chat.Id
        let actions: [RoomToolbarAction]
    }

    /// Replaces the quick replies of a room.
    struct RoomSuggestionsUpdate {
        let chatId: Chat.Id
        let suggestions: [RoomSuggestion]
    }

    /// Replaces the slash commands of a room.
    struct RoomCommandsUpdate {
        let chatId: Chat.Id
        let commands: [RoomCommand]
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
