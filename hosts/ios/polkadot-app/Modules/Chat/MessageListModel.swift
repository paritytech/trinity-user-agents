import Foundation

struct MessageListModel {
    let allMessages: [Chat.LocalMessage]
    let messagesById: [Chat.MessageId: Int]
    let orderedSections: [MessageListSection]
    let messagesBySection: [MessageListSection: [Chat.LocalMessage]]
    let reactionsByMessageId: [Chat.MessageId: [Chat.MessageReactionAggregate]]
    let latestEditByMessageId: [Chat.MessageId: Chat.EditedMessage]
    let initiallyUnreadMessage: Chat.MessageId?
    let firstUnreadMessageId: Chat.MessageId?
    let oldestNewReactionTargetMessageId: Chat.MessageId?
    let newMessageCount: Int

    var hasNewMessages: Bool {
        newMessageCount > 0
    }

    func getMessage(by messageId: Chat.MessageId) -> Chat.LocalMessage? {
        guard let index = messagesById[messageId] else {
            return nil
        }

        return allMessages[index]
    }

    func getLatestEdit(for messageId: Chat.MessageId) -> Chat.EditedMessage? {
        latestEditByMessageId[messageId]
    }

    func isEdited(_ messageId: Chat.MessageId) -> Bool {
        latestEditByMessageId[messageId] != nil
    }
}

enum MessageListSection: Hashable {
    case today
    case yesterday
    case other(date: Date)
}
