package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.MessageRevision
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductOutboxEntry
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot

/**
 * The window with the entries of the outbox as messages of the user, in both room modes, each at its time as the native
 * chat orders its messages. An entry shows the native pending state while the host sends it: processing while its
 * attachment uploads, new after that, and failed only when it cannot go. An edit and a reaction that wait for the
 * message show on it. Once the window has the message under the same id, the window shows it. [preview] gives the
 * resource that the bubble draws before the host sends the attachment.
 */
fun ChatFeedSnapshot.withOutbox(
    entries: List<ProductOutboxEntry>,
    preview: (ProductOutboxEntry) -> ChatResourceModel,
): ChatFeedSnapshot {
    val known = messages.mapTo(HashSet()) { it.id }
    val waiting = entries.filter { it.id !in known }
    if (waiting.isEmpty()) return this

    val merged = messages.toMutableList()
    // The window holds the newest message first. An entry goes before the first message that is not newer.
    waiting.sortedBy { it.createdAt }.forEach { entry ->
        val message = entry.toPendingMessage(preview)
        val index = merged.indexOfFirst { it.timestamp <= message.timestamp }.takeIf { it != -1 } ?: merged.size
        merged.add(index, message)
    }

    return copy(
        messages = merged,
        reactions = reactions + waiting.mapNotNull { entry -> entry.reaction?.let { entry.id to listOf(entry.userReaction(it)) } },
        revisions = revisions + waiting.mapNotNull { entry -> entry.revision(preview)?.let { entry.id to it } },
        productStatuses = productStatuses + waiting.associate { it.id to it.productStatus },
    )
}

/** The entry as a message of the user in the native pending state. */
internal fun ProductOutboxEntry.toPendingMessage(preview: (ProductOutboxEntry) -> ChatResourceModel) = ChatMessage(
    id = id,
    chatId = chatId,
    timestamp = createdAt,
    origin = ChatMessageOrigin.User,
    // As a native message, it keeps its first text; an edit shows through its revision.
    content = displayContent(text, preview),
    status = when {
        isFailed -> ChatMessage.Status.DELIVERY_FAILED
        // As a native attachment, which is processing until its upload ends.
        !isText && hopFile == null -> ChatMessage.Status.PROCESSING
        else -> ChatMessage.Status.NEW
    },
    replyToMessageId = replyTo,
)

/** The welcome message of a chat request draws as the native request message. */
private fun ProductOutboxEntry.displayContent(text: String?, preview: (ProductOutboxEntry) -> ChatResourceModel): ChatMessage.Content {
    val content = copy(text = text).content(if (isText) null else preview(this))
    if (!request || content !is ChatMessage.Content.Text) return content
    return ChatMessage.Content.ChatRequest(ChatMessage.Content.RichText(content.text, emptyList()))
}

private fun ProductOutboxEntry.revision(preview: (ProductOutboxEntry) -> ChatResourceModel): MessageRevision? {
    val edited = editedText ?: return null
    return MessageRevision(messageId = id, content = displayContent(edited, preview), chatId = chatId, timestamp = editedAt ?: createdAt)
}

private fun ProductOutboxEntry.userReaction(emoji: String) =
    ChatMessageReaction(messageId = id, content = ChatMessageReactionContent(emoji), origin = ChatMessageOrigin.User, timestamp = createdAt)

private val ProductOutboxEntry.productStatus: ProductMessageStatusModel
    get() = if (isFailed) ProductMessageStatusModel.FAILED else ProductMessageStatusModel.PREPARING
