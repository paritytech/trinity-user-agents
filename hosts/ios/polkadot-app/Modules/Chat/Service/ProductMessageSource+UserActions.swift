import Foundation
import StructuredConcurrency
import TrUAPIHost

// MARK: - User actions

extension ProductMessageSource {
    static let localIdPrefix = "local:"

    func perform(_ action: ChatUserAction) async -> ChatSourceActionOutcome {
        switch action {
        case let .newMessage(body, replyTo):
            return await send(body: body, replyTo: replyTo)
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
            modify(messageId) { $0.adjustReaction(emoji, by: productUserParticipantId, placed: placed) }
            return await ask(action) { [weak self] in
                await self?.put(original)
            }
        case .deleteMessage,
             .seen,
             .retry:
            return await ask(action) {}
        }
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

    func send(body: ChatMessageBody, replyTo: String?) async -> ChatSourceActionOutcome {
        if !isAtNewestEnd {
            await reload()
        }

        let localId = Self.localIdPrefix + UUID().uuidString
        messages[localId] = ChatMessage(
            messageId: localId,
            origin: ChatMessageOrigin(participantId: productUserParticipantId),
            timestamp: now(),
            body: body,
            status: .preparing,
            replyTo: replyTo,
            editedAt: nil,
            reactions: []
        )
        pending.append(localId)

        return await deliver(localId)
    }

    /// Sends a pending message to the product, and replaces it with the message that the product made.
    func deliver(_ localId: String) async -> ChatSourceActionOutcome {
        guard let message = messages[localId] else {
            return .refused(reason: nil)
        }

        emit()

        do {
            let item = try await querying.userAction(ProductChatUserActionRequest(
                roomId: roomId,
                action: .newMessage(body: message.body, replyTo: message.replyTo)
            ))

            switch item {
            case let .messageCreated(created):
                pending.removeAll { $0 == localId }
                messages[localId] = nil
                insertLive(created)
            case .acknowledged:
                modify(localId) { $0.status = .sent }
            }

            emit()
            return .done
        } catch let ChatQueryError.actionRefused(.rejected(reason)) {
            // 02-DESIGN §13 item 9: a rejected send goes away and the user sees the reason.
            pending.removeAll { $0 == localId }
            messages[localId] = nil
            emit()
            return .refused(reason: reason)
        } catch {
            logger.warning("Send to room \(roomId) failed: \(error)")
            modify(localId) { $0.status = .failed }
            noteUnavailable(after: error)
            emit()
            return .failed
        }
    }

    func retry(_ messageId: String) async -> ChatSourceActionOutcome {
        guard let message = messages[messageId], message.status == .failed else {
            return .refused(reason: nil)
        }

        modify(messageId) { $0.status = .preparing }

        if pending.contains(messageId) {
            // The product never confirmed the message, so the host sends it again.
            return await deliver(messageId)
        }

        return await ask(.retry(messageId: messageId)) { [weak self] in
            await self?.markFailed(messageId)
        }
    }

    func markFailed(_ messageId: String) {
        modify(messageId) { $0.status = .failed }
    }

    func delete(_ messageId: String) async -> ChatSourceActionOutcome {
        let removed = remove(messageId)
        emit()

        // A message that the product never confirmed exists only in the window.
        guard !messageId.hasPrefix(Self.localIdPrefix) else {
            return .done
        }

        return await ask(.deleteMessage(messageId: messageId)) { [weak self] in
            if let removed {
                await self?.putBack(removed)
            }
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
    var orderedIds: [String] {
        pages.flatMap(\.ids) + pending
    }

    /// Notes the newest of the messages as seen, and tells the product when it asks for `Seen`.
    func markSeen(_ messageIds: Set<Chat.MessageId>) {
        let ids = orderedIds
        guard
            let newestIndex = ids.lastIndex(where: { messageIds.contains($0) && !$0.hasPrefix(Self.localIdPrefix) }),
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

        let reported = reportedSeenMessageId.flatMap { messages[$0] }
        if isNewer(newest, than: reportedSeenMessageId, timestamp: reported?.timestamp, in: ids) {
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
