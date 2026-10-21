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
    /// The state of each end of the window. Only a product-source room pages.
    var paging: ChatPagingState = .complete
    var readiness: ChatRoomReadiness = .ready

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

    /// True when the room holds messages outside the window, so a reply can quote a message that is not here.
    var holdsMessagesOutsideWindow: Bool {
        paging.older != .end || paging.newer != .end
    }
}

enum MessageListSection: Hashable {
    case today
    case yesterday
    case other(date: Date)
}

extension MessageListModel {
    /// The model with an edit for each message that the product edited. The edit carries the text that the
    /// message shows now, so the room screen marks the message as edited.
    func addingEdits(_ editedAt: [Chat.MessageId: Chat.Timestamp]) -> MessageListModel {
        var edits = latestEditByMessageId

        for (messageId, timestamp) in editedAt {
            guard let message = getMessage(by: messageId), let text = message.content.editableText else {
                continue
            }

            edits[messageId] = Chat.EditedMessage(
                messageId: messageId,
                newContent: .init(text: text, attachments: nil),
                origin: message.origin,
                chatId: message.chatId,
                timestamp: timestamp
            )
        }

        var model = MessageListModel(
            allMessages: allMessages,
            messagesById: messagesById,
            orderedSections: orderedSections,
            messagesBySection: messagesBySection,
            reactionsByMessageId: reactionsByMessageId,
            latestEditByMessageId: edits,
            initiallyUnreadMessage: initiallyUnreadMessage,
            firstUnreadMessageId: firstUnreadMessageId,
            oldestNewReactionTargetMessageId: oldestNewReactionTargetMessageId,
            newMessageCount: newMessageCount
        )
        model.paging = paging
        model.readiness = readiness
        return model
    }
}

private extension Chat.LocalMessage.Content {
    /// The text of a text message or a reply. Other content kinds show no edit text.
    var editableText: String? {
        switch self {
        case let .text(text):
            text
        case let .reply(reply):
            reply.ownContent.text
        default:
            nil
        }
    }
}
