package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.MessageRevision
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductOutboxEntry
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot

/**
 * The window with the entries of the outbox as messages of the user, each at its time. An entry shows the pending state
 * while the host sends it, and the failed state only when it cannot go. An edit and a reaction that wait for the message
 * show on it. Once the window has the message under the same id, the window shows it.
 */
fun ChatFeedSnapshot.withOutbox(entries: List<ProductOutboxEntry>): ChatFeedSnapshot {
    val known = messages.mapTo(HashSet()) { it.id }
    val waiting = entries.filter { it.id !in known }
    if (waiting.isEmpty()) return this

    val merged = messages.toMutableList()
    // The window holds the newest message first. An entry goes before the first message that is not newer.
    waiting.sortedBy { it.createdAt }.forEach { entry ->
        val message = entry.toPendingMessage()
        val index = merged.indexOfFirst { it.timestamp <= message.timestamp }.takeIf { it != -1 } ?: merged.size
        merged.add(index, message)
    }

    return copy(
        messages = merged,
        reactions = reactions + waiting.mapNotNull { entry -> entry.reaction?.let { entry.id to listOf(entry.userReaction(it)) } },
        revisions = revisions + waiting.mapNotNull { entry -> entry.revision()?.let { entry.id to it } },
    )
}

/** The entry as a message of the user in the pending state. */
internal fun ProductOutboxEntry.toPendingMessage() = ChatMessage(
    id = id,
    chatId = chatId,
    timestamp = createdAt,
    origin = ChatMessageOrigin.User,
    // The message keeps its first text. An edit shows through its revision.
    content = content(),
    status = if (isFailed) ChatMessage.Status.DELIVERY_FAILED else ChatMessage.Status.NEW,
    replyToMessageId = replyTo,
)

private fun ProductOutboxEntry.revision(): MessageRevision? {
    val edited = editedText ?: return null
    return MessageRevision(messageId = id, content = ChatMessage.Content.Text(edited), chatId = chatId, timestamp = editedAt ?: createdAt)
}

private fun ProductOutboxEntry.userReaction(emoji: String) =
    ChatMessageReaction(messageId = id, content = ChatMessageReactionContent(emoji), origin = ChatMessageOrigin.User, timestamp = createdAt)
