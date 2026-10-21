package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductOutboxEntry
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot

/**
 * The window with the entries of the outbox as messages of the user at the newest end, in both room modes. An entry
 * shows `Preparing` while the host sends it and `Failed` after a failure. [preview] gives the resource that the bubble
 * draws before the preimage exists.
 */
fun ChatFeedSnapshot.withOutbox(
    entries: List<ProductOutboxEntry>,
    preview: (ProductOutboxEntry) -> ChatResourceModel,
): ChatFeedSnapshot {
    if (entries.isEmpty()) return this

    val pending = entries.sortedByDescending { it.createdAt }.map { entry ->
        ChatMessage(
            id = entry.id,
            chatId = entry.chatId,
            timestamp = entry.createdAt,
            origin = ChatMessageOrigin.User,
            content = entry.content(preview(entry)),
            status = if (entry.isFailed) ChatMessage.Status.DELIVERY_FAILED else ChatMessage.Status.PROCESSING,
            replyToMessageId = entry.replyTo,
        )
    }
    val statuses = entries.associate { entry ->
        entry.id to if (entry.isFailed) ProductMessageStatusModel.FAILED else ProductMessageStatusModel.PREPARING
    }

    return copy(messages = pending + messages, productStatuses = productStatuses + statuses)
}
