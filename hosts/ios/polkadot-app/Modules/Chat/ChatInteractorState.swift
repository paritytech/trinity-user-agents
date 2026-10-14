import Foundation
import Foundation_iOS

// return first unread message id when on messages update
// the first unread message id is set only once - needed for New Messages header
// a message can be marked as seen only once per session
// reaction and edits are marked as read when corresponding message is requested to be marked as read
actor ChatInteractorState {
    private var initiallyUnreadMessage: UncertainStorage<Chat.MessageId?> = .undefined
    private var newIncomingMessageIds: Set<Chat.MessageId> = []
    private var markedAsSeenMessageIds: Set<Chat.MessageId> = []
    private var reactionEventsByMessageId: [Chat.MessageId: Set<Chat.MessageId>] = [:]
    private var editsEventsByMessageId: [Chat.MessageId: Set<Chat.MessageId>] = [:]
    private var pendingReactionTargetMessageId: Chat.MessageId?
    var model: MessageListModel?

    func onMessagesUpdate(_ messages: [Chat.LocalMessage]) -> MessageListModel {
        let unreadMessages = findAllUnreadMessages(from: messages)
        let unreadMessageIds = unreadMessages.map(\.messageId)
        let unreadIndependentMessageIds = unreadMessages
            .filter(\.isIndependentMessageInChat)
            .map(\.messageId)

        newIncomingMessageIds = Set(unreadMessageIds)

        switch initiallyUnreadMessage {
        case .undefined:
            initiallyUnreadMessage = .defined(unreadIndependentMessageIds.first)
        case .defined:
            break
        }

        // Find all target message IDs that are either unread themselves or have unread metadata (reactions/edits)
        let unreadTargetIds = Set(unreadMessages.map { resolveTargetMessageId(from: $0) })

        // firstUnreadMessageId should be the ID of the chronologically earliest visible message
        // that has any unread state (itself or its metadata).
        let firstUnreadMessage = messages.first { !$0.isSystem && unreadTargetIds.contains($0.messageId) }
        let firstUnreadMessageId = firstUnreadMessage?.messageId

        // Set reaction target when a new unread reaction appears, keep until target is marked as read
        if let newTarget = findOldestNewReactionTargetMessageId(from: unreadMessages) {
            if pendingReactionTargetMessageId == nil {
                pendingReactionTargetMessageId = newTarget
            }
        }

        let model = prepareMessageListModel(
            from: messages,
            initiallyUnreadMessage: initiallyUnreadMessage.valueWhenDefined(else: nil),
            firstUnreadMessageId: firstUnreadMessageId,
            oldestNewReactionTargetMessageId: pendingReactionTargetMessageId,
            newMessagesCount: unreadIndependentMessageIds.count
        )

        self.model = model
        reactionEventsByMessageId = buildReactionEventsByMessageId(from: messages)
        editsEventsByMessageId = buildEditEventsByMessageId(from: messages)

        return model
    }

    /// Finds all edits and reactions for the provided messages and returns Set of **Chat.MessageId**
    /// - Parameter messageIds: Message identifiers to mark as read
    /// - Returns: all connected message ids (edits, reactions) that were marked as seen
    func markAsSeen(
        messageIds: [Chat.MessageId]
    ) -> Set<Chat.MessageId> {
        guard let model else {
            return []
        }

        let maxIndex = messageIds.compactMap { model.messagesById[$0] }.max()
        guard let maxIndex else {
            return []
        }

        let messagesToMark = model.allMessages[0 ... maxIndex]

        let allMessageIds = messagesToMark.flatMap { message in
            let messageId = message.messageId
            return (reactionEventsByMessageId[messageId] ?? [])
                .union(editsEventsByMessageId[messageId] ?? [])
                .union([messageId])
        }
        .toSet()

        let messageIdsToMark = allMessageIds
            .intersection(newIncomingMessageIds)
            .filter {
                markedAsSeenMessageIds.insert($0).inserted
            }

        if let target = pendingReactionTargetMessageId, messageIds.contains(target) {
            pendingReactionTargetMessageId = nil
        }
        return messageIdsToMark
    }
}

private extension ChatInteractorState {
    var calendar: Calendar {
        .current
    }

    func prepareMessageListModel(
        from messages: [Chat.LocalMessage],
        initiallyUnreadMessage: Chat.MessageId?,
        firstUnreadMessageId: Chat.MessageId?,
        oldestNewReactionTargetMessageId: Chat.MessageId?,
        newMessagesCount: Int
    ) -> MessageListModel {
        var messagesById = [Chat.MessageId: Int]()
        var messagesBySection = [MessageListSection: [Chat.LocalMessage]]()
        var orderedSections = [MessageListSection]()
        var lastSection: MessageListSection?

        for (offset, message) in messages.enumerated() {
            messagesById[message.messageId] = offset

            let date = Date(timeIntervalSince1970: TimeInterval(message.timestamp) / 1_000)
            let section = makeSection(date: date)

            if section != lastSection {
                orderedSections.append(section)
                lastSection = section
            }

            messagesBySection[section, default: []].append(message)
        }

        let reactions = aggregateReactionsFromMessages(messages)
        let latestEdits = aggregateLatestEditsFromMessages(messages)

        return MessageListModel(
            allMessages: messages,
            messagesById: messagesById,
            orderedSections: orderedSections,
            messagesBySection: messagesBySection,
            reactionsByMessageId: reactions,
            latestEditByMessageId: latestEdits,
            initiallyUnreadMessage: initiallyUnreadMessage,
            firstUnreadMessageId: firstUnreadMessageId,
            oldestNewReactionTargetMessageId: oldestNewReactionTargetMessageId,
            newMessageCount: newMessagesCount
        )
    }

    func makeSection(date: Date) -> MessageListSection {
        if calendar.isDateInToday(date) {
            .today
        } else if calendar.isDateInYesterday(date) {
            .yesterday
        } else {
            .other(date: calendar.startOfDay(for: date))
        }
    }

    func aggregateReactionsFromMessages(
        _ messages: [Chat.LocalMessage]
    ) -> [String: [Chat.MessageReactionAggregate]] {
        let currentUserOrigin = Chat.LocalMessage.Origin.user

        var aggregatedReactions: [String: [Chat.MessageReactionAggregate]] = [:]
        for message in messages where !message.reactions.isEmpty {
            let aggregates = Chat.MessageReactionAggregate.aggregate(
                reactions: message.reactions,
                currentUserOrigin: currentUserOrigin
            )
            aggregatedReactions[message.messageId] = aggregates
        }

        return aggregatedReactions
    }

    func aggregateLatestEditsFromMessages(
        _ messages: [Chat.LocalMessage]
    ) -> [String: Chat.EditedMessage] {
        var latestEdits: [String: Chat.EditedMessage] = [:]

        for message in messages {
            guard case let .edited(editedContent) = message.content else {
                continue
            }

            let edit = Chat.EditedMessage(
                messageId: editedContent.messageId,
                newContent: editedContent.newContent,
                origin: message.origin,
                chatId: message.chatId,
                timestamp: message.timestamp
            )

            // Keep the latest edit (highest timestamp) just to track that message was edited
            if let existingEdit = latestEdits[editedContent.messageId] {
                if edit.timestamp > existingEdit.timestamp {
                    latestEdits[editedContent.messageId] = edit
                }
            } else {
                latestEdits[editedContent.messageId] = edit
            }
        }

        return latestEdits
    }

    func findAllUnreadMessages(from messages: [Chat.LocalMessage]) -> [Chat.LocalMessage] {
        messages.filter { message in
            guard case .incoming(.new) = message.status else {
                return false
            }
            return true
        }
    }

    func resolveTargetMessageId(from message: Chat.LocalMessage) -> Chat.MessageId {
        switch message.content {
        case let .reacted(content):
            content.messageId
        case let .edited(content):
            content.messageId
        case let .reactionRemoved(content):
            content.messageId
        default:
            message.messageId
        }
    }

    func findOldestNewReactionTargetMessageId(from unreadMessages: [Chat.LocalMessage]) -> Chat.MessageId? {
        for message in unreadMessages {
            if case let .reacted(content) = message.content {
                return content.messageId
            }
        }
        return nil
    }

    func buildReactionEventsByMessageId(from messages: [Chat.LocalMessage]) -> [Chat.MessageId: Set<Chat.MessageId>] {
        messages.reduce(into: [:]) { accum, message in
            switch message.content {
            case let .reacted(content):
                accum[content.messageId] = accum[content.messageId, default: []].union([message.messageId])
            case let .reactionRemoved(content):
                accum[content.messageId] = accum[content.messageId, default: []].union([message.messageId])
            default:
                break
            }
        }
    }

    func buildEditEventsByMessageId(from messages: [Chat.LocalMessage]) -> [Chat.MessageId: Set<Chat.MessageId>] {
        messages.reduce(into: [:]) { accum, message in
            switch message.content {
            case let .edited(content):
                accum[content.messageId] = accum[content.messageId, default: []].union([message.messageId])
            default:
                break
            }
        }
    }
}
