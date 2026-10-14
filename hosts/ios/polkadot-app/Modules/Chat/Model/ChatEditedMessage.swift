import Foundation
import Operation_iOS

extension Chat {
    struct EditedMessage: Equatable {
        let messageId: MessageId
        let newContent: ChatRemoteMessageContent.RichText
        let origin: Chat.LocalMessage.Origin
        let chatId: Chat.Id
        let timestamp: Timestamp

        init(
            messageId: MessageId,
            newContent: ChatRemoteMessageContent.RichText,
            origin: Chat.LocalMessage.Origin,
            chatId: Chat.Id,
            timestamp: Timestamp
        ) {
            self.messageId = messageId
            self.newContent = newContent
            self.origin = origin
            self.chatId = chatId
            self.timestamp = timestamp
        }

        var newText: String? {
            newContent.text
        }
    }
}

extension Chat.EditedMessage: Operation_iOS.Identifiable {
    var identifier: String {
        "\(messageId)_edit_\(timestamp)"
    }
}
