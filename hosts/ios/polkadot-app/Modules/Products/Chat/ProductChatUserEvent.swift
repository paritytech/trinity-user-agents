import Foundation

/// What the user did in a product room that the host stores, as the product receives it on
/// `action_subscribe` (02-DESIGN §3.3.9, §13 item 11). Each message id is the id that the product knows.
enum ProductChatUserEvent: Equatable, Sendable {
    /// The host gave the id. The user posts text only, because a product room takes no attachment yet (P6.7).
    case messagePosted(messageId: String, text: String, replyTo: String?)
    case edited(messageId: String, text: String)
    case deleted(messageId: String)
    case reacted(messageId: String, emoji: String, placed: Bool)
    case seen(messageId: String)
}

extension ProductChatUserEvent {
    /// The event of a message that the user saved in a room, or nil for a message that is no user action.
    init?(userMessage message: Chat.LocalMessage) {
        let chatId = message.chatId
        let productId = { (storedId: Chat.MessageId) in Chat.ProductMessageId.product(storedId, in: chatId) }

        switch message.content {
        case let .text(text),
             let .extensionActionResponse(text, _):
            self = .messagePosted(messageId: message.messageId, text: text, replyTo: nil)
        case let .reply(reply):
            guard let text = reply.ownContent.text else { return nil }
            self = .messagePosted(messageId: message.messageId, text: text, replyTo: productId(reply.messageId))
        case let .edited(edit):
            guard let text = edit.newContent.text else { return nil }
            self = .edited(messageId: productId(edit.messageId), text: text)
        case let .reacted(reaction):
            self = .reacted(messageId: productId(reaction.messageId), emoji: reaction.emoji, placed: true)
        case let .reactionRemoved(reaction):
            self = .reacted(messageId: productId(reaction.messageId), emoji: reaction.emoji, placed: false)
        default:
            return nil
        }
    }
}
