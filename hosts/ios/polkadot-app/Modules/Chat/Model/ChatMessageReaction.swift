import Foundation
import Operation_iOS

extension Chat {
    struct MessageReaction: Equatable {
        let messageId: MessageId
        let emoji: String
        let origin: Chat.LocalMessage.Origin
        let chatId: Chat.Id
        let timestamp: Timestamp

        init(
            messageId: MessageId,
            emoji: String,
            origin: Chat.LocalMessage.Origin,
            chatId: Chat.Id,
            timestamp: Timestamp
        ) {
            self.messageId = messageId
            self.emoji = emoji
            self.origin = origin
            self.chatId = chatId
            self.timestamp = timestamp
        }
    }
}

extension Chat.MessageReaction: Operation_iOS.Identifiable {
    var identifier: String {
        "\(messageId)_\(emoji)_\(origin.rawType)"
    }
}
