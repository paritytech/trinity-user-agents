import Foundation

/// What the user did in a product room that the host stores, as the product receives it on
/// `action_subscribe` (02-DESIGN §3.3.9, §13 item 11). Each message id is the id that the product knows.
enum ProductChatUserEvent: Equatable, Sendable {
    /// The host gave the id.
    case messagePosted(messageId: String, text: String, replyTo: String?)
    /// The user posted an inline attachment (P6.7). The host gave the id.
    case attachmentPosted(messageId: String, body: Chat.LocalMessage.Content.ProductBody)
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
        case let .productBody(body):
            // The stager sends a staged attachment itself (P10.3). While the host stages it, the message is `new`
            // or `failed`. After that, its media holds a preimage key.
            guard !body.holdsPreimageMedia, message.status != .outgoing(.new), message.status != .outgoing(.failed)
            else {
                return nil
            }
            self = .attachmentPosted(messageId: message.messageId, body: body)
        default:
            return nil
        }
    }
}

extension Chat.LocalMessage.Content.ProductBody {
    /// True when a media item of the body is a preimage key. A user body has one only after the host staged it.
    var holdsPreimageMedia: Bool {
        let sources: [String] =
            switch self {
            case let .richText(richText): richText.media
            case let .file(file): [file.url]
            case .actions: []
            }
        return sources.contains { source in
            if case .preimage = ChatResource(source: source) { true } else { false }
        }
    }
}
