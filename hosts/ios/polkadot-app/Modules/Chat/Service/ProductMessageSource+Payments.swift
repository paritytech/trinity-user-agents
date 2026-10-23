import Foundation
import TrUAPIHost

// MARK: - Payments

extension ProductMessageSource {
    /// Shows the payments of the user that the product does not have yet, and follows the payments of the room. A
    /// payment that the host keeps waits at the end of the window, as a native payment message waits to be sent. When
    /// the product has it, the message of the product takes its place under the same id.
    func followPayments() async {
        guard let payments else { return }

        let events = await payments.events()
        for payment in await payments.payments(productId: productId, roomId: roomId) {
            if let state = payment.state {
                paymentStates[payment.messageId] = state
            }
            if !payment.incoming, payment.delivery != .delivered {
                show(payment)
            }
        }

        paymentsTask = Task { [weak self] in
            for await event in events {
                await self?.apply(event)
            }
        }
    }

    func apply(_ event: ProductChatPayments.Event) {
        switch event {
        case let .staged(payment):
            guard isOfRoom(payment) else { return }
            show(payment)
        case let .delivered(payment, created):
            guard isOfRoom(payment) else { return }
            delivered(payment, created: created)
        case let .refused(payment):
            guard isOfRoom(payment) else { return }
            modify(payment.messageId) { $0.status = .failed }
        case let .state(messageId, state):
            guard paymentStates[messageId] != state else { return }
            paymentStates[messageId] = state
            guard messages[messageId] != nil else { return }
        }

        emit()
    }

    /// The product has the payment: its message takes the place of the waiting one.
    private func delivered(_ payment: ProductChatPayment, created: ChatMessage?) {
        pending.removeAll { $0 == payment.messageId }
        if let created {
            insertLive(created)
        } else if let shown = messages[payment.messageId] {
            messages[payment.messageId] = nil
            var sent = shown
            sent.status = .sent
            insertLive(sent)
        }
    }

    private func isOfRoom(_ payment: ProductChatPayment) -> Bool {
        payment.productId == productId && payment.roomId == roomId
    }

    private func show(_ payment: ProductChatPayment) {
        // The product can give the message before the host learns that it has it.
        if pages.contains(where: { $0.ids.contains(payment.messageId) }) {
            return
        }

        if !pending.contains(payment.messageId) {
            pending.append(payment.messageId)
        }

        messages[payment.messageId] = ChatMessage(
            messageId: payment.messageId,
            origin: ChatMessageOrigin(participantId: productUserParticipantId),
            timestamp: payment.timestamp,
            body: .payment(ChatPaymentRecord(amount: String(payment.totalValue), legacy: false)),
            status: payment.delivery == .refused ? .failed : .preparing,
            replyTo: nil,
            replyPreview: nil,
            editedAt: nil,
            reactions: [],
            request: false
        )
    }
}
