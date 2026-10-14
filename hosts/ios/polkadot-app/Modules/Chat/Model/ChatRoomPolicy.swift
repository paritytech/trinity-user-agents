import Foundation

/// What a product room offers. The value comes from the declarations of the product and, for each aspect
/// that the product does not declare, from the host defaults of 02-DESIGN §4.
///
/// The policy reads the footer and the chat-list preview of a room. The room configuration stays opaque
/// bytes until the Swift bindings of P2.10 exist, so the aspects that it sets use the defaults.
struct ChatRoomPolicy: Equatable {
    enum MessageStore: Equatable {
        /// The host database keeps the messages. `post_message` and feed updates write to it.
        case host
        /// The product keeps the messages, and the host keeps a page cache (`product_source`). No room has
        /// this store until the configuration of a room decodes (P2.10).
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

    enum SlashCommands: Equatable {
        /// Any `/name` is a command. This is the behaviour until a product can declare its commands.
        case anyName
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
        slashCommands: .anyName,
        activity: nil,
        hostAppliesUserActions: true
    )

    /// The policy of a room with the footer and the chat-list preview that the product set, if any.
    init(footer: Chat.RoomFooter?, preview: Chat.RoomPreview = .hostDefault) {
        let defaults = Self.defaults
        self.init(
            messageStore: defaults.messageStore,
            listPreview: ListPreview(preview: preview),
            listBadge: ListBadge(badge: preview.badge),
            listOrder: ListOrder(isPinned: preview.order == .pinToTop, timestamp: preview.timestamp),
            reportsSeen: defaults.reportsSeen,
            messageMenu: defaults.messageMenu,
            participants: defaults.participants,
            offersReactionDetails: defaults.offersReactionDetails,
            toolbar: defaults.toolbar,
            footer: footer == .empty ? .empty : defaults.footer,
            placeholder: defaults.placeholder,
            suggestions: defaults.suggestions,
            slashCommands: defaults.slashCommands,
            activity: defaults.activity,
            hostAppliesUserActions: defaults.hostAppliesUserActions
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
            ChatRoomPolicy(footer: roomFooter, preview: roomDeclarations?.preview ?? .hostDefault)
        }
    }
}
