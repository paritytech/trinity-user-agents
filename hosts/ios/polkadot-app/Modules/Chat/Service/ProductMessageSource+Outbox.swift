import Foundation
import TrUAPIHost

// MARK: - Messages of the user

extension ProductMessageSource {
    /// Shows the messages of the room that the outbox holds, and follows their changes (P10.3). A message of the
    /// outbox waits at the end of the window. When the product has it, the message of the product takes its place
    /// under the same id.
    func followOutbox() async {
        let events = await outbox.events()
        for message in await outbox.entries(productId: productId, roomId: roomId) {
            show(message)
        }

        outboxTask = Task { [weak self] in
            for await event in events {
                await self?.apply(event)
            }
        }
    }

    /// Puts a text of the user in the outbox, under an id that the host chose, and shows it at once.
    func send(text: String, replyTo: String?, request: Bool = false) async -> ChatSourceActionOutcome {
        if !isAtNewestEnd {
            await reload()
        }

        do {
            let message = try await outbox.send(
                text: text,
                replyTo: replyTo,
                request: request,
                productId: productId,
                roomId: roomId
            )
            show(message)
            emit()
            return .done
        } catch {
            logger.error("Room \(roomId) did not keep a message: \(error)")
            return .failed
        }
    }

    /// Puts the photo or the video that the user picked in the outbox. The caption goes with it.
    func sendMedia(_ richText: Chat.LocalMessage.Content.RichText) async -> ChatSourceActionOutcome {
        guard case let .localUploadable(file)? = richText.attachments?.first else {
            return .refused(reason: nil)
        }

        if !isAtNewestEnd {
            await reload()
        }

        do {
            let message = try await outbox.send(
                ProcessedAttachment(fileId: file.relativeLocalPath, fileUrl: URL(fileURLWithPath: ""), meta: file.meta),
                caption: richText.text,
                productId: productId,
                roomId: roomId
            )
            show(message)
            emit()
            return .done
        } catch {
            logger.error("Room \(roomId) did not keep a photo or a video: \(error)")
            return .failed
        }
    }

    func apply(_ event: ProductOutboxEvent) {
        switch event {
        case let .changed(message):
            guard isOfRoom(message) else { return }
            show(message)
        case let .delivered(message, created):
            guard isOfRoom(message) else { return }
            pending.removeAll { $0 == message.id }
            uploads[message.id] = nil
            if let created {
                insertLive(created)
            } else if let shown = messages[message.id] {
                messages[message.id] = nil
                var sent = shown
                sent.status = .sent
                insertLive(sent)
            }
        }

        emit()
    }

    /// Waits until the product has a message of the user that the outbox holds. An action on the message goes after
    /// it, as the native chat sends it after the message. Gives false when the product refused the message.
    func waitUntilDelivered(_ messageId: String) async -> Bool {
        guard pending.contains(messageId) else {
            return true
        }

        return await outbox.waitUntilDelivered(messageId)
    }

    private func isOfRoom(_ message: ProductOutgoingMessage) -> Bool {
        message.productId == productId && message.roomId == roomId
    }

    private func show(_ message: ProductOutgoingMessage) {
        // The product can give the message before the outbox tells that it has it.
        if pages.contains(where: { $0.ids.contains(message.id) }) {
            return
        }

        if !pending.contains(message.id) {
            pending.append(message.id)
        }

        switch message.content {
        case let .text(text, replyTo, request):
            guard messages[message.id] == nil else { return }
            messages[message.id] = ChatMessage(
                messageId: message.id,
                origin: ChatMessageOrigin(participantId: productUserParticipantId),
                timestamp: message.createdAt,
                body: .text(text: text),
                status: .preparing,
                replyTo: replyTo,
                replyPreview: replyTo.flatMap { messages[$0] }.map {
                    ChatReplyPreview(origin: $0.origin, body: $0.body)
                },
                editedAt: nil,
                reactions: [],
                request: request
            )
        case .media:
            uploads[message.id] = message
        }
    }
}
