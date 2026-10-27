import Foundation
import StructuredConcurrency
import TrUAPIHost

// MARK: - Window changes

extension ProductMessageSource {
    var orderedIds: [String] {
        pages.flatMap(\.ids)
    }

    func insertLive(_ message: ChatMessage) {
        if contains(message.messageId) {
            messages[message.messageId] = message
            return
        }

        // The newest pages are out of the window. The message comes back with them.
        guard isAtNewestEnd else {
            return
        }

        if pages.isEmpty {
            pages = [Page(anchor: .before(cursor: nil), next: nil, ids: [])]
        }

        var page = pages[pages.count - 1]
        let lastEarlier = page.ids.lastIndex { id in
            (messages[id]?.timestamp ?? 0) <= message.timestamp
        }
        let index = lastEarlier.map { $0 + 1 } ?? 0
        page.ids.insert(message.messageId, at: index)
        pages[pages.count - 1] = page
        messages[message.messageId] = message

        dropOlderPagesIfNeeded()
    }

    func modify(_ messageId: String, change: (inout ChatMessage) -> Void) {
        guard var message = messages[messageId] else {
            return
        }

        change(&message)
        messages[messageId] = message
    }

    func restore(from entry: ProductRoomCacheEntry) {
        for message in entry.messages {
            messages[message.messageId] = message
        }

        pages = [Page(anchor: .before(cursor: nil), next: entry.next, ids: entry.messages.map(\.messageId))]
        seenMessageId = entry.seenMessageId
        seenTimestamp = entry.seenTimestamp
        reportedSeenMessageId = entry.reportedSeenMessageId
    }

    func storeCache() async {
        guard hasFirstPage, isAtNewestEnd, let newest = pages.last else {
            return
        }

        let entry = ProductRoomCacheEntry(
            messages: newest.ids.suffix(Int(settings.pageSize)).compactMap { messages[$0] },
            next: newest.ids.count > Int(settings.pageSize) ? nil : newest.next,
            seenMessageId: seenMessageId,
            seenTimestamp: seenTimestamp,
            reportedSeenMessageId: reportedSeenMessageId
        )
        await cache.store(entry, productId: productId, roomId: roomId)
    }
}

// MARK: - Snapshot

extension ProductMessageSource {
    func emit() {
        let ids = orderedIds
        let seenIndex = seenMessageId.flatMap { ids.firstIndex(of: $0) }
        var editedAt: [Chat.MessageId: Chat.Timestamp] = [:]
        var replyPreviews: [Chat.MessageId: Chat.LocalMessage] = [:]
        var unseenReactionTargets: [Chat.MessageId] = []

        let localMessages = ids.enumerated().compactMap { index, id -> Chat.LocalMessage? in
            guard let message = messages[id] else {
                return nil
            }

            if let edit = message.editedAt {
                editedAt[id] = edit
            }

            if let replyTo = message.replyTo, messages[replyTo] == nil,
               let quoted = message.replyPreviewMessage(chatId: chatId, extensionId: productId) {
                replyPreviews[replyTo] = quoted
            }

            if message.hasUnseenReaction {
                unseenReactionTargets.append(id)
            }

            // The user saw the message on this device before the product marked it read.
            let isSeen: Bool =
                if let seenIndex {
                    index <= seenIndex
                } else if let seenTimestamp {
                    message.timestamp <= seenTimestamp
                } else {
                    false
                }

            return message.localMessage(chatId: chatId, extensionId: productId, isSeen: isSeen)
        }

        subject.send(MessageListSnapshot(
            messages: localMessages,
            paging: paging,
            readiness: readiness,
            editedAt: editedAt,
            replyPreviews: replyPreviews,
            unseenReactionTargets: unseenReactionTargets
        ))
    }

    var paging: ChatPagingState {
        let older: ChatPagingState.Edge =
            if olderTask != nil || (!hasFirstPage && readiness == .loading) {
                .loading
            } else if olderFailed {
                .failed
            } else if olderAnchor != nil {
                .more
            } else {
                .end
            }

        let newer: ChatPagingState.Edge =
            if newerTask != nil {
                .loading
            } else if newerFailed {
                .failed
            } else if !droppedNewer.isEmpty {
                .more
            } else {
                .end
            }

        return ChatPagingState(older: older, newer: newer)
    }
}

extension ChatMessage {
    /// A reaction of the contact that the user has not seen yet.
    var hasUnseenReaction: Bool {
        reactions.contains(where: \.unseen)
    }

    /// The replied message as the room screen quotes it, when the window does not hold it.
    func replyPreviewMessage(chatId: Chat.Id, extensionId: ChatExtension.Id) -> Chat.LocalMessage? {
        guard let replyTo, let replyPreview else {
            return nil
        }

        return ChatMessage(
            messageId: replyTo,
            origin: replyPreview.origin,
            timestamp: timestamp,
            body: replyPreview.body,
            status: .read,
            replyTo: nil,
            replyPreview: nil,
            editedAt: nil,
            reactions: [],
            request: false
        )
        .localMessage(chatId: chatId, extensionId: extensionId)
    }

    /// Counts a reaction once for each participant. The user reaction sets `reactedByUser`.
    mutating func adjustReaction(_ emoji: String, by participantId: String, placed: Bool) {
        let isUser = participantId == productUserParticipantId
        var summaries = reactions

        if let index = summaries.firstIndex(where: { $0.emoji == emoji }) {
            var summary = summaries[index]

            // The product can repeat the reaction of the user that the window shows already.
            if isUser, summary.reactedByUser == placed {
                return
            }

            if placed {
                summary.count += 1
            } else {
                summary.count = max(summary.count, 1) - 1
            }

            if isUser {
                summary.reactedByUser = placed
            } else if placed {
                summary.unseen = true
            }

            if summary.count == .zero {
                summaries.remove(at: index)
            } else {
                summaries[index] = summary
            }
        } else if placed {
            summaries.append(ChatReactionSummary(emoji: emoji, count: 1, reactedByUser: isUser, unseen: !isUser))
        }

        reactions = summaries
    }
}
