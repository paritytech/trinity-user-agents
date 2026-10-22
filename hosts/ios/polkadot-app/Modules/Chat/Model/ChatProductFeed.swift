import Foundation

extension Chat {
    /// The reserved participant ids of Chat Extension v2 (02-DESIGN §3.3.3).
    enum ProductParticipant {
        /// The user of the host.
        static let user = "user"
        /// The product itself.
        static let product = "product"
    }

    /// The status of a message as a product gives it (`ChatMessageStatus`).
    enum ProductMessageStatus: Equatable {
        case preparing
        case sent
        case delivered
        case read
        case failed
    }

    /// A new message that a product pushes into a room that the host stores.
    struct ProductFeedMessage: Equatable {
        /// The id that the product picked. It is unique in the room only.
        let messageId: String
        let participantId: String
        /// Milliseconds since the Unix epoch.
        let timestamp: UInt64
        let content: LocalMessage.Content
        /// The id of the message that this message replies to, as the product names it.
        let replyTo: String?
    }

    /// One change to the messages of a room that the host stores (`ChatFeedUpdate`).
    enum ProductFeedUpdate: Equatable {
        case new(ProductFeedMessage)
        case edited(messageId: String, content: LocalMessage.Content, editedAt: UInt64)
        /// The message draws again in place, with no "edited".
        case replaced(ProductFeedMessage)
        case reactionPlaced(messageId: String, participantId: String, emoji: String)
        case reactionRemoved(messageId: String, participantId: String, emoji: String)
        case statusChanged(messageId: String, status: ProductMessageStatus)
    }

    /// Why the host refuses a feed update (`NativeChatFeedRejection`).
    enum ProductFeedError: Error, Equatable {
        case unknownMessage
        case duplicateMessage
        /// The product tried to write as the user, or to change a message that it did not write.
        case notOwner
    }

    /// The store gives each message one id across all chats, but a product picks the id of a message in one
    /// room only. So the store keeps a message that a product pushes under an id that starts with the chat id.
    /// A message that the host made (a message of the user, a `post_message` message) keeps the id that the
    /// host gave it, and the product knows it under that id.
    enum ProductMessageId {
        static func stored(_ productMessageId: String, in chatId: Chat.Id) -> MessageId {
            prefix(of: chatId) + productMessageId
        }

        /// The id under which the product knows a stored message.
        static func product(_ storedId: MessageId, in chatId: Chat.Id) -> String {
            let prefix = prefix(of: chatId)
            return storedId.hasPrefix(prefix) ? String(storedId.dropFirst(prefix.count)) : storedId
        }

        /// The stored ids that a product id can name, the id of a pushed message first.
        static func candidates(_ productMessageId: String, in chatId: Chat.Id) -> [MessageId] {
            [stored(productMessageId, in: chatId), productMessageId]
        }

        private static func prefix(of chatId: Chat.Id) -> String {
            chatId.rawRepresentation + "/"
        }
    }
}
