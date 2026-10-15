import Foundation
import TrUAPIHost

// MARK: - Staged attachments

extension ProductMessageSource {
    /// Shows the outbox entries of the room, and follows their changes (P10.3). An entry is a message of the user
    /// that waits at the end of the window. When the product made the message, its message takes the place of the
    /// entry.
    func followOutbox() async {
        guard let stager else {
            return
        }

        let events = await stager.events()
        for (entry, preview) in await stager.previews(productId: productId, roomId: roomId) {
            showOutboxEntry(entry, preview: preview)
        }

        outboxTask = Task { [weak self] in
            for await event in events {
                await self?.apply(event)
            }
        }
    }

    func apply(_ event: ProductOutboxEvent) {
        switch event {
        case let .changed(entry, preview):
            guard isOfRoom(entry) else { return }
            showOutboxEntry(entry, preview: preview)
        case let .delivered(entry, created, body):
            guard isOfRoom(entry) else { return }
            if let created {
                pending.removeAll { $0 == entry.id }
                messages[entry.id] = nil
                insertLive(created)
            } else {
                // The product acknowledged the message and made no message of its own.
                modify(entry.id) {
                    $0.body = body
                    $0.status = .sent
                }
            }
        case let .removed(entry, _):
            guard isOfRoom(entry) else { return }
            remove(entry.id)
        }

        emit()
    }

    private func isOfRoom(_ entry: ProductOutboxEntry) -> Bool {
        entry.productId == productId && entry.roomId == roomId && entry.store == .product
    }

    private func showOutboxEntry(_ entry: ProductOutboxEntry, preview: ChatMessageBody) {
        guard entry.store == .product else { return }

        if messages[entry.id] == nil {
            pending.append(entry.id)
        }
        messages[entry.id] = ChatMessage(
            messageId: entry.id,
            origin: ChatMessageOrigin(participantId: productUserParticipantId),
            timestamp: entry.createdAt,
            body: preview,
            status: entry.isFailed ? .failed : .preparing,
            replyTo: nil,
            editedAt: nil,
            reactions: []
        )
    }
}
