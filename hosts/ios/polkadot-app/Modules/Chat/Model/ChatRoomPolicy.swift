import Foundation

/// What a product room offers. The value comes from the declarations of the product and, for each aspect
/// that the product does not declare, from the host defaults of 02-DESIGN §4.
///
/// The policy reads the footer, the chat-list badge, the configuration, the features and the contact of a room. The
/// call lives in ``ChatRoomCallStore``, because the host does not keep it.
struct ChatRoomPolicy: Equatable {
    enum MessageStore: Equatable {
        /// The host database keeps the messages. `post_message` and feed updates write to it.
        case host
        /// The product keeps the messages, and the host keeps a page cache (`product_source`).
        case product
    }

    enum ListBadge: Equatable {
        /// The host counts the messages after the last message that the user saw.
        case hostUnreadCount
        /// The product asks for no badge.
        case hidden
        /// The number that the product set. Zero shows no badge.
        case count(Int)
    }

    /// What the room shows below its messages (`set_room_footer`).
    enum Footer: Equatable {
        case input
        case empty
    }

    let messageStore: MessageStore
    let listBadge: ListBadge
    let reportsSeen: Bool
    let offersReactionDetails: Bool
    /// The header buttons that the host draws: the call buttons of `features.calls` (07-LATER §11.2).
    let toolbar: [Chat.PeerAction]
    let footer: Footer
    /// The host applies reply, edit and reactions to its own store and reports them to the product.
    let hostAppliesUserActions: Bool
    /// Goes up by one at each configuration that the product sets, so an open room knows to reload.
    var configGeneration: Int64 = 0
    /// The optional features that the product declared (`set_room_features`). The value keeps a feature that this
    /// build does not serve. Read ``ChatHostCapabilities`` before you draw a feature.
    var features: Chat.RoomFeatures = .hostDefault
    /// The person the room is a conversation with. The room then looks like a person chat of the native chat.
    var contact: Chat.RoomContact?
}

extension ChatRoomPolicy {
    /// The policy of a room that declares nothing.
    static let defaults = ChatRoomPolicy(
        messageStore: .host,
        listBadge: .hostUnreadCount,
        reportsSeen: false,
        offersReactionDetails: false,
        toolbar: [],
        footer: .input,
        hostAppliesUserActions: true
    )

    /// The policy of a room from what its product declared. A nil value of a declaration uses the default.
    init(footer: Chat.RoomFooter?, declarations: Chat.RoomDeclarations?) {
        let defaults = Self.defaults
        let config = declarations?.effectiveConfig ?? .hostDefault

        self.init(
            messageStore: config.productSource ? .product : .host,
            listBadge: ListBadge(badge: declarations?.preview.badge ?? .hostDefault),
            reportsSeen: config.reportSeen,
            offersReactionDetails: config.reactionDetails,
            toolbar: Self.callButtons(for: declarations?.features),
            footer: footer == .empty ? .empty : defaults.footer,
            hostAppliesUserActions: !config.productSource,
            configGeneration: declarations?.configGeneration ?? 0,
            features: declarations?.features ?? .hostDefault,
            contact: declarations?.contact
        )
    }

    /// The call buttons of the header (07-LATER §11.2): one button for each kind of call that the room declares, when
    /// this build serves `call_buttons`.
    static func callButtons(
        for features: Chat.RoomFeatures?,
        serves: (String) -> Bool = ChatHostCapabilities.serves
    ) -> [Chat.PeerAction] {
        guard let calls = features?.calls, serves(ChatHostCapabilities.callButtons) else { return [] }
        return (calls.audio ? [.audioCall] : []) + (calls.video ? [.videoCall] : [])
    }

    /// True when the header of the room shows a call button.
    var hasCallButtons: Bool {
        toolbar.contains { $0 == .audioCall || $0 == .videoCall }
    }

    /// The user sends photos and videos to a HOP node, as the native chat sends them: the room declares
    /// `hop_attachments` and this host lists it.
    var sendsMedia: Bool {
        features.hopAttachments && ChatHostCapabilities.serves(ChatHostCapabilities.hopAttachments)
    }

    /// The number that the chat list shows on the row of the room.
    func listBadgeCount(hostUnreadCount: Int) -> Int {
        switch listBadge {
        case .hostUnreadCount:
            hostUnreadCount
        case let .count(count):
            count
        case .hidden:
            0
        }
    }
}

private extension ChatRoomPolicy.ListBadge {
    init(badge: Chat.RoomBadge) {
        switch badge {
        case .hostDefault: self = .hostUnreadCount
        case .none: self = .hidden
        case let .unreadCount(count): self = .count(Int(max(count, 0)))
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

    /// The contact of a room of the chat product. Nil for any other chat.
    var roomContact: Chat.RoomContact? {
        roomDeclarations?.contact
    }

    /// The seed of the color of the avatar: the account of the contact of a room, as the native chat colors the
    /// chat with that account.
    var avatarColorSeed: String {
        (roomContact?.personChatId ?? chatId).colorSeed
    }
}

extension ChatRoomPolicy.Footer {
    /// True when the footer is the host input, so the user can reply and react.
    var showsInput: Bool {
        if case .input = self { true } else { false }
    }
}
