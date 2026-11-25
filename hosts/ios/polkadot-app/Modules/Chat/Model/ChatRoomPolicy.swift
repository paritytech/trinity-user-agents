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
    let footer: Footer
    /// Goes up by one at each configuration that the product sets, so an open room knows to reload.
    var configGeneration: Int64 = 0
}

extension ChatRoomPolicy {
    /// The policy of a room that declares nothing.
    static let defaults = ChatRoomPolicy(
        messageStore: .host,
        footer: .input
    )

    /// A room whose product keeps the messages tells the product which messages the user saw.
    var reportsSeen: Bool {
        messageStore == .product
    }

    /// The policy of a room from what its product declared. A nil value of a declaration uses the default.
    init(footer: Chat.RoomFooter?, declarations: Chat.RoomDeclarations?) {
        let config = declarations?.effectiveConfig ?? .hostDefault

        self.init(
            messageStore: config.productSource ? .product : .host,
            footer: Self.footer(of: footer),
            configGeneration: declarations?.configGeneration ?? 0
        )
    }
}

private extension ChatRoomPolicy {
    /// The host input shows only for the text-input footer, which is also the footer of a room
    /// that set none. A docked card takes the place of the input, as an empty footer does.
    static func footer(of footer: Chat.RoomFooter?) -> Footer {
        switch footer {
        case nil, .textInput:
            defaults.footer
        case .empty, .card:
            .empty
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

extension ChatRoomPolicy.Footer {
    /// True when the footer is the host input, so the user can reply and react.
    var showsInput: Bool {
        if case .input = self { true } else { false }
    }
}
