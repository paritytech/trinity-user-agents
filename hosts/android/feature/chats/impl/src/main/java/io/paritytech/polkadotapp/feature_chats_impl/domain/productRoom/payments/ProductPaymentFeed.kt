package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot

/**
 * The window with the status that the host keeps for each payment of the room, by message id, as the native chat
 * draws the status that its coinage flow writes into a payment message. A payment of the user that has not reached
 * the product yet shows at its time in the native pending state: new while it goes, failed when it cannot go.
 */
fun ChatFeedSnapshot.withPayments(payments: List<ProductChatPayment>): ChatFeedSnapshot {
    if (payments.isEmpty()) return this
    val byId = payments.associateBy { it.messageId }

    val shown = messages.map { it.withPaymentStatus(byId[it.id]) }
    val known = shown.mapTo(HashSet()) { it.id }
    val waiting = payments.filter { !it.incoming && it.isUndelivered && it.messageId !in known }

    val merged = shown.toMutableList()
    // The window holds the newest message first. A payment goes before the first message that is not newer.
    waiting.sortedBy { it.timestamp }.forEach { payment ->
        val message = payment.toPendingMessage()
        val index = merged.indexOfFirst { it.timestamp <= message.timestamp }.takeIf { it != -1 } ?: merged.size
        merged.add(index, message)
    }

    return copy(
        messages = merged,
        productStatuses = productStatuses + waiting.associate { it.messageId to it.productStatus },
        replyQuotes = replyQuotes.mapValues { (id, quote) -> quote.withPaymentStatus(byId[id]) },
    )
}

/** The message with the status that the host keeps for its payment. Other messages stay. */
fun ChatMessage.withPaymentStatus(payment: ProductChatPayment?): ChatMessage {
    val content = content as? ChatMessage.Content.CoinagePayment ?: return this
    if (payment == null) return this
    return copy(content = content.copy(status = payment.status))
}

/** A payment of the user that waits for the product, as the native chat shows a payment message it sends. */
fun ProductChatPayment.toPendingMessage() = ChatMessage(
    id = messageId,
    chatId = chatId,
    timestamp = timestamp,
    origin = ChatMessageOrigin.User,
    content = ChatMessage.Content.CoinagePayment(totalValue = totalValue, coinKeys = emptyList(), status = status),
    status = if (delivery == ProductChatPayment.Delivery.REFUSED) ChatMessage.Status.DELIVERY_FAILED else ChatMessage.Status.NEW,
)

private val ProductChatPayment.productStatus: ProductMessageStatusModel
    get() = if (delivery == ProductChatPayment.Delivery.REFUSED) ProductMessageStatusModel.FAILED else ProductMessageStatusModel.PREPARING
