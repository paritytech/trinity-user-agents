import Foundation
import StructuredConcurrency
import TrUAPIHost

// MARK: - User actions

extension ProductMessageSource {
    // swiftlint:disable:next cyclomatic_complexity
    func perform(_ action: ChatUserAction) async -> ChatSourceActionOutcome {
        switch action {
        case let .newMessage(body, replyTo, _, _):
            guard case let .text(text) = body else {
                return .refused(reason: nil)
            }

            return await send(text: text, replyTo: replyTo)
        case let .editMessage(messageId, body):
            guard let original = messages[messageId] else {
                return .refused(reason: nil)
            }

            modify(messageId) {
                $0.body = body
                $0.editedAt = now()
            }
            return await ask(action) { [weak self] in
                await self?.put(original)
            }
        case let .reactionPlaced(messageId, emoji),
             let .reactionRemoved(messageId, emoji):
            guard let original = messages[messageId] else {
                return .refused(reason: nil)
            }

            let placed = if case .reactionPlaced = action { true } else { false }
            modify(messageId) { message in
                // The user has one reaction on a message: a new one replaces the one before.
                if placed {
                    for other in message.reactions where other.reactedByUser && other.emoji != emoji {
                        message.adjustReaction(other.emoji, by: productUserParticipantId, placed: false)
                    }
                }
                message.adjustReaction(emoji, by: productUserParticipantId, placed: placed)
            }
            return await ask(action) { [weak self] in
                await self?.put(original)
            }
        case .seen:
            return await ask(action) {}
        case .sendPayment,
             .acceptRequest,
             .declineRequest,
             .block,
             .unblock,
             .leave:
            // This host does not send a payment or a contact action to the product.
            return .refused(reason: nil)
        }
    }

    /// Sends a text of the user under an id that the host chose. The window shows the message that the product
    /// made with it.
    func send(text: String, replyTo: String?) async -> ChatSourceActionOutcome {
        if !isAtNewestEnd {
            await reload()
        }

        let action = ChatUserAction.newMessage(
            body: .text(text: text),
            replyTo: replyTo,
            messageId: UUID().uuidString,
            timestamp: now()
        )

        do {
            let answer = try await querying.userAction(ProductChatUserActionRequest(roomId: roomId, action: action))
            if case let .messageCreated(created) = answer {
                insertLive(created)
                emit()
            }
            return .done
        } catch {
            logger.warning("Message of room \(roomId) failed: \(error)")
            noteUnavailable(after: error)
            emit()
            return .refused(reason: Self.reason(of: error))
        }
    }

    /// The body of an edit: a text keeps its kind, and a caption keeps the media of its message.
    func editedBody(of messageId: String, text: String) -> ChatMessageBody {
        guard case let .richText(_, media)? = messages[messageId]?.body, !media.isEmpty else {
            return .text(text: text)
        }

        return .richText(text: text, media: media)
    }

    func put(_ message: ChatMessage) {
        guard messages[message.messageId] != nil else {
            return
        }

        messages[message.messageId] = message
    }

    /// Sends an action whose change the window shows already. `revert` undoes the change when the product
    /// refuses the action or does not answer.
    func ask(_ action: ChatUserAction, revert: @escaping () async -> Void) async -> ChatSourceActionOutcome {
        emit()

        do {
            _ = try await querying.userAction(ProductChatUserActionRequest(roomId: roomId, action: action))
            return .done
        } catch {
            logger.warning("User action of room \(roomId) failed: \(error)")
            await revert()
            noteUnavailable(after: error)
            emit()
            return .refused(reason: Self.reason(of: error))
        }
    }

    func noteUnavailable(after error: Error) {
        if Self.isUnavailable(error) {
            readiness = .unavailable
        }
    }

    static func reason(of error: Error) -> String? {
        if case let ChatQueryError.actionRefused(.rejected(reason)) = error {
            return reason
        }

        return nil
    }
}

// MARK: - Seen

extension ProductMessageSource {
    /// Notes the newest of the messages as seen, and tells the product when it asks for `Seen`.
    func markSeen(_ messageIds: Set<Chat.MessageId>) {
        let ids = orderedIds
        guard
            let newestIndex = ids.lastIndex(where: { messageIds.contains($0) }),
            let newest = messages[ids[newestIndex]]
        else {
            return
        }

        if isNewer(newest, than: seenMessageId, timestamp: seenTimestamp, in: ids) {
            seenMessageId = newest.messageId
            seenTimestamp = newest.timestamp
            emit()
        }

        guard settings.reportsSeen else {
            return
        }

        // The native chat marks the reactions of every message up to the newest seen one as seen. The product
        // marks the reactions of the room seen with `Seen`.
        // A message of the contact that the product still gives as unread goes again with the next `Seen`.
        let seesUnread = ids[...newestIndex].contains { id in
            guard let message = messages[id] else { return false }
            return message.hasUnseenReaction || message.isUnreadByUser
        }
        let reported = reportedSeenMessageId.flatMap { messages[$0] }
        if seesUnread || isNewer(newest, than: reportedSeenMessageId, timestamp: reported?.timestamp, in: ids) {
            if seenToReport != newest.messageId {
                failedSeenAttempts = 0
            }
            seenToReport = newest.messageId
            reportSeenIfNeeded()
        }
    }

    func isNewer(
        _ message: ChatMessage,
        than otherId: String?,
        timestamp: UInt64?,
        in ids: [String]
    ) -> Bool {
        guard let otherId, otherId != message.messageId else {
            return otherId == nil
        }

        if let index = ids.firstIndex(of: message.messageId), let otherIndex = ids.firstIndex(of: otherId) {
            return index > otherIndex
        }

        return message.timestamp > (timestamp ?? 0)
    }

    /// Sends one `Seen` at a time. A newer message that the user sees in the meantime goes next.
    func reportSeenIfNeeded() {
        guard !isReportingSeen, let messageId = seenToReport else {
            return
        }

        isReportingSeen = true
        seenToReport = nil

        Task { [weak self, querying, roomId] in
            do {
                _ = try await querying.userAction(ProductChatUserActionRequest(
                    roomId: roomId,
                    action: .seen(messageId: messageId)
                ))
            } catch {
                self?.logger.warning("Seen of room \(roomId) failed: \(error)")
                await self?.didFailToReportSeen(messageId)
                return
            }

            await self?.didReportSeen(messageId)
        }
    }

    func didReportSeen(_ messageId: String) async {
        reportedSeenMessageId = messageId
        isReportingSeen = false
        failedSeenAttempts = 0
        reportSeenIfNeeded()
        await storeCache()
    }

    /// The product did not take the `Seen`, for example because its engine starts. The product keeps the old
    /// unread count until it gets a `Seen`, so the source sends it again after `seenRetryDelay`, at most
    /// `seenAttempts` times. The next message that the user sees, or the next open of the room, sends it again too.
    func didFailToReportSeen(_ messageId: String) async {
        isReportingSeen = false
        failedSeenAttempts += 1

        if seenToReport == nil {
            seenToReport = messageId
        }

        guard failedSeenAttempts < settings.seenAttempts else {
            seenToReport = nil
            failedSeenAttempts = 0
            return
        }

        try? await Task.sleep(for: settings.seenRetryDelay)
        reportSeenIfNeeded()
    }
}
