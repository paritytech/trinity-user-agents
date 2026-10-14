import Foundation

enum ChatMessageComparator {
    static func timestampThenOrderComparator(
        message1: Chat.LocalMessage,
        message2: Chat.LocalMessage
    ) -> Bool {
        guard message1.timestamp != message2.timestamp else {
            return message1.messageId.localizedCompare(message2.messageId) == .orderedAscending
        }

        return message1.timestamp < message2.timestamp
    }
}
