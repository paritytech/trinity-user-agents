import Foundation

/// What a product room offers. The value comes from the declarations of the product and, for each aspect
/// that the product does not declare, from the host defaults of 02-DESIGN §4.
///
/// Today the only declaration that the host stores is the footer of a room. All other aspects use the
/// defaults until the host stores the room configuration (P3.1, P3.2).
struct ChatRoomPolicy: Equatable {
    enum MessageStore: Equatable {
        /// The host database keeps the messages. `post_message` and feed updates write to it.
        case host
    }

    enum ListPreview: Equatable {
        /// The last message gives the text and the order of the row.
        case lastMessage
    }

    enum ListBadge: Equatable {
        /// The host counts the messages after the last message that the user saw.
        case hostUnreadCount
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

    /// The policy of a room with the footer that the product set, if any.
    init(footer: Chat.RoomFooter?) {
        let defaults = Self.defaults
        self.init(
            messageStore: defaults.messageStore,
            listPreview: defaults.listPreview,
            listBadge: defaults.listBadge,
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

    /// The number that the chat list shows on the row of the room.
    func listBadgeCount(hostUnreadCount: Int) -> Int {
        switch listBadge {
        case .hostUnreadCount:
            hostUnreadCount
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
            ChatRoomPolicy(footer: roomFooter)
        }
    }
}
