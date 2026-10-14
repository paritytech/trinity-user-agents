import Foundation

/// What a product room offers. The value comes from the declarations of the product and, for each aspect
/// that the product does not declare, from the host defaults of 02-DESIGN §4.
///
/// The policy reads the footer, the chat-list preview, the configuration, the toolbar, the suggestions and the
/// commands of a room. The activity line lives in ``ChatRoomActivityStore``, because the host does not keep it.
struct ChatRoomPolicy: Equatable {
    enum MessageStore: Equatable {
        /// The host database keeps the messages. `post_message` and feed updates write to it.
        case host
        /// The product keeps the messages, and the host keeps a page cache (`product_source`).
        case product
    }

    enum ListPreview: Equatable {
        /// The last message gives the text of the row.
        case lastMessage
        /// The text that the product set. `ongoing` draws it as an activity that continues.
        case text(String, ongoing: Bool)
    }

    enum ListBadge: Equatable {
        /// The host counts the messages after the last message that the user saw.
        case hostUnreadCount
        /// The product asks for no badge.
        case hidden
        /// The number that the product set. Zero shows no badge.
        case count(Int)
        case image(Chat.RoomImage)
    }

    /// Where the row of the room goes in the chat list.
    struct ListOrder: Equatable {
        /// A pinned room goes above every room that is not pinned.
        let isPinned: Bool
        /// The sort key in milliseconds since the Unix epoch. Nil uses the last message.
        let timestamp: Int64?

        static let lastMessage = ListOrder(isPinned: false, timestamp: nil)
    }

    enum BuiltInMessageAction: CaseIterable, Equatable {
        case copy
        case reply
        case edit
        case delete
        case react
        case retry
    }

    struct MessageMenu: Equatable {
        let builtIns: Set<BuiltInMessageAction>
        /// The emoji that the reaction picker shows. Nil means all emoji.
        let allowedReactions: [String]?
    }

    enum Participant: Equatable {
        case user
        /// The product, with the name and icon of its manifest.
        case product
    }

    struct Input: Equatable {
        let allowsPhoto: Bool
        let allowsVideo: Bool
        let allowsFile: Bool
        let allowsPayment: Bool
        /// The text of the empty input. Nil means the host text.
        let placeholder: String?
    }

    enum Footer: Equatable {
        case input(Input)
        case empty
    }

    /// Only a declared command name after `/` is a command. Any other `/text` is a message
    /// (02-DESIGN §13 item 12).
    enum SlashCommands: Equatable {
        case declared(Set<String>)

        func isCommand(_ name: String) -> Bool {
            switch self {
            case let .declared(names): names.contains(name)
            }
        }
    }

    /// A question that the host asks the product about the room (02-DESIGN §3.2).
    enum ProductQuery: Hashable, CaseIterable {
        case participants
        case messageActions
        case participantActions
    }

    let messageStore: MessageStore
    let listPreview: ListPreview
    let listBadge: ListBadge
    let listOrder: ListOrder
    let reportsSeen: Bool
    let messageMenu: MessageMenu
    let participants: [Participant]
    let offersReactionDetails: Bool
    let toolbar: [Chat.PeerAction]
    let footer: Footer
    /// The text of an empty room. Nil means the host text.
    let placeholder: String?
    let suggestions: [String]
    let slashCommands: SlashCommands
    let activity: String?
    /// The host applies reply, edit, delete and reactions to its own store and reports them to the product.
    let hostAppliesUserActions: Bool
    /// The questions that the room declares. Each other aspect uses the host default.
    var productQueries: Set<ProductQuery> = []
    /// Goes up by one at each configuration that the product sets, so an open room knows to reload.
    var configGeneration: Int64 = 0
    /// The buttons of the room toolbar that the product set (`set_room_toolbar`).
    var toolbarActions: [Chat.RoomToolbarAction] = []
    /// The quick replies above the input that the product set (`set_room_suggestions`).
    var roomSuggestions: [Chat.RoomSuggestion] = []
    /// The slash commands that the product declares (`set_room_commands`), for the catalogue of the input.
    var roomCommands: [Chat.RoomCommand] = []
}

extension ChatRoomPolicy {
    static let defaultInput = Input(
        allowsPhoto: true,
        allowsVideo: true,
        allowsFile: false,
        allowsPayment: false,
        placeholder: nil
    )

    /// The policy of a room that declares nothing.
    static let defaults = ChatRoomPolicy(
        messageStore: .host,
        listPreview: .lastMessage,
        listBadge: .hostUnreadCount,
        listOrder: .lastMessage,
        reportsSeen: false,
        messageMenu: MessageMenu(builtIns: Set(BuiltInMessageAction.allCases), allowedReactions: nil),
        participants: [.user, .product],
        offersReactionDetails: false,
        toolbar: [],
        footer: .input(defaultInput),
        placeholder: nil,
        suggestions: [],
        slashCommands: .declared([]),
        activity: nil,
        hostAppliesUserActions: true
    )

    /// The policy of a room with the footer and the chat-list preview that the product set, if any.
    init(footer: Chat.RoomFooter?, preview: Chat.RoomPreview = .hostDefault) {
        self.init(footer: footer, declarations: nil, preview: preview)
    }

    /// The policy of a room from what its product declared. A nil value of a declaration uses the default.
    init(footer: Chat.RoomFooter?, declarations: Chat.RoomDeclarations?) {
        self.init(footer: footer, declarations: declarations, preview: declarations?.preview ?? .hostDefault)
    }

    private init(footer: Chat.RoomFooter?, declarations: Chat.RoomDeclarations?, preview: Chat.RoomPreview) {
        let defaults = Self.defaults
        let config = declarations?.effectiveConfig ?? .hostDefault

        self.init(
            messageStore: config.productSource ? .product : .host,
            listPreview: ListPreview(preview: preview),
            listBadge: ListBadge(badge: preview.badge),
            listOrder: ListOrder(isPinned: preview.order == .pinToTop, timestamp: preview.timestamp),
            reportsSeen: config.reportSeen,
            messageMenu: MessageMenu(
                builtIns: defaults.messageMenu.builtIns,
                allowedReactions: config.allowedReactions
            ),
            participants: defaults.participants,
            offersReactionDetails: config.reactionDetails,
            toolbar: defaults.toolbar,
            footer: Footer(declared: declarations?.footerConfig, legacy: footer) ?? defaults.footer,
            placeholder: config.placeholder,
            suggestions: declarations?.suggestions?.map(\.text) ?? defaults.suggestions,
            slashCommands: .declared(Set(declarations?.commands?.map(\.name) ?? [])),
            activity: defaults.activity,
            hostAppliesUserActions: !config.productSource,
            productQueries: ProductQuery.declared(by: config),
            configGeneration: declarations?.configGeneration ?? 0,
            toolbarActions: declarations?.toolbar ?? [],
            roomSuggestions: declarations?.suggestions ?? [],
            roomCommands: declarations?.commands ?? []
        )
    }

    /// The input that the room screen shows.
    ///
    /// This host cannot send an attachment into a product room yet (P6.7), so the photo and video defaults
    /// show no attachment button.
    var peerInput: Chat.PeerMetadataInput {
        switch footer {
        case .empty:
            .empty
        case let .input(input):
            .inputField(.init(canPay: input.allowsPayment, canAttachFile: false))
        }
    }

    /// The built-in Retry exists only in a room whose product keeps the messages (02-DESIGN §13 item 9).
    var offersRetry: Bool {
        messageStore == .product && messageMenu.builtIns.contains(.retry)
    }

    /// The built-in Delete. A product menu can hide it when message menus exist (P6.1).
    var offersDelete: Bool {
        messageMenu.builtIns.contains(.delete)
    }

    /// The number that the chat list shows on the row of the room.
    func listBadgeCount(hostUnreadCount: Int) -> Int {
        switch listBadge {
        case .hostUnreadCount:
            hostUnreadCount
        case let .count(count):
            count
        case .hidden,
             .image:
            0
        }
    }
}

private extension ChatRoomPolicy.Footer {
    /// The declared footer wins over the footer of the first version. A product draws no custom footer on this
    /// host yet (P6.6), so the bridge refuses one and a stored one reads as no declaration.
    init?(declared: Chat.RoomFooterConfig?, legacy: Chat.RoomFooter?) {
        switch declared {
        case let .input(input):
            self = .input(ChatRoomPolicy.Input(
                allowsPhoto: input.photo,
                allowsVideo: input.video,
                allowsFile: input.file,
                allowsPayment: input.payment != nil,
                placeholder: input.placeholder
            ))
        case .empty:
            self = .empty
        case .custom,
             nil:
            guard legacy == .empty else { return nil }
            self = .empty
        }
    }
}

private extension ChatRoomPolicy.ProductQuery {
    static func declared(by config: Chat.RoomConfig) -> Set<Self> {
        Set(allCases.filter { query in
            switch query {
            case .participants: config.participants
            case .messageActions: config.messageActions
            case .participantActions: config.participantActions
            }
        })
    }
}

private extension ChatRoomPolicy.ListPreview {
    init(preview: Chat.RoomPreview) {
        if let text = preview.text {
            self = .text(text, ongoing: preview.ongoing)
        } else {
            self = .lastMessage
        }
    }
}

private extension ChatRoomPolicy.ListBadge {
    init(badge: Chat.RoomBadge) {
        switch badge {
        case .hostDefault: self = .hostUnreadCount
        case .none: self = .hidden
        case let .unreadCount(count): self = .count(Int(max(count, 0)))
        case let .image(image): self = .image(image)
        }
    }
}

extension Chat.LocalModel {
    /// The policy of a product room, or nil for a person chat.
    var roomPolicy: ChatRoomPolicy? {
        switch peer {
        case .person:
            nil
        case .chatExtension:
            ChatRoomPolicy(footer: roomFooter, declarations: roomDeclarations)
        }
    }
}
