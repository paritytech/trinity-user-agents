import Foundation

/// What a product room offers. The value comes from the declarations of the product and, for each aspect
/// that the product does not declare, from the host defaults.
///
/// The policy reads the footer and the configuration of a room.
struct ChatRoomPolicy: Equatable {
    enum MessageStore: Equatable {
        /// The host database keeps the messages. `post_message` and feed updates write to it.
        case host
        /// The product keeps the messages, and the host keeps a page cache (`product_source`).
        case product
    }

    /// What the room shows below its messages (`set_room_footer`).
    enum Footer: Equatable {
        case input
        case empty
    }

    let messageStore: MessageStore
    let reportsSeen: Bool
    let footer: Footer
    /// Goes up by one at each configuration that the product sets, so an open room knows to reload.
    var configGeneration: Int64 = 0
}

extension ChatRoomPolicy {
    /// The policy of a room that declares nothing.
    static let defaults = ChatRoomPolicy(
        messageStore: .host,
        reportsSeen: false,
        footer: .input
    )

    /// The policy of a room from what its product declared. A nil value of a declaration uses the default.
    init(footer: Chat.RoomFooter?, declarations: Chat.RoomDeclarations?) {
        let defaults = Self.defaults
        let config = declarations?.effectiveConfig ?? .hostDefault

        self.init(
            messageStore: config.productSource ? .product : .host,
            reportsSeen: config.reportSeen,
            footer: footer == .empty ? .empty : defaults.footer,
            configGeneration: declarations?.configGeneration ?? 0
        )
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

extension ChatRoomPolicy.Footer {
    /// True when the footer is the host input, so the user can reply and react.
    var showsInput: Bool {
        if case .input = self { true } else { false }
    }
}
