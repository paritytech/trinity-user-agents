import Foundation

extension Chat {
    /// The menu of one message, as the product answers `message_actions_query` (`ChatMessageMenu`).
    enum ProductMessageMenu: Equatable, Sendable {
        /// Only the entries of the product.
        case custom([ProductMenuAction])
        /// The host entries without `hidden`, then the entries of the product in their own section.
        case extendBuiltIn([ProductMenuAction], hidden: Set<ChatRoomPolicy.BuiltInMessageAction>)
    }

    /// The menu of one participant, as the product answers `participant_actions_query` (`ChatParticipantMenu`).
    enum ProductParticipantMenu: Equatable, Sendable {
        /// Only the entries of the product. The menu has no host entry.
        case custom([ProductMenuAction])
        /// The host entry for the kind of the participant, then the entries of the product.
        case extendBuiltIn([ProductMenuAction])
    }
}

extension Chat.ProductResource {
    /// The resource as the chat resource resolver loads it.
    var chatResource: ChatResource {
        switch self {
        case let .inline(mimeType, data): .inline(mimeType: mimeType, data: data)
        case let .preimage(key): .preimage(key: key)
        }
    }
}

extension [Chat.ProductMenuAction] {
    /// The entries in the order that the menu draws them: a destructive entry goes after the other entries.
    var inMenuOrder: [Chat.ProductMenuAction] {
        filter { !$0.destructive } + filter(\.destructive)
    }
}
