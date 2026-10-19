package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import uniffi.truapi.ChatMessage
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatPageAnchor
import uniffi.truapi.ChatReactionSummary
import uniffi.truapi.ProductChatMessagesQueryItem
import uniffi.truapi.ProductChatSearchQueryItem

/**
 * The messages of one room of the mock product, oldest first, as a product that keeps its messages holds them.
 *
 * A cursor is the id of the message at the edge of the page that the host has, so a message that arrives between two
 * pages does not move the next page. Not thread-safe: the runtime guards it.
 */
class MockChatConversation(messages: List<ChatMessage>) {
    private val messages = messages.toMutableList()

    val all: List<ChatMessage> get() = messages.toList()

    fun message(messageId: String): ChatMessage? = messages.firstOrNull { it.messageId == messageId }

    fun page(anchor: ChatPageAnchor, size: Int): ProductChatMessagesQueryItem = when (anchor) {
        is ChatPageAnchor.Before -> {
            val end = anchor.cursor?.let(::indexOf) ?: messages.size
            val start = maxOf(end - size, 0)
            val page = messages.subList(start, end).toList()
            ProductChatMessagesQueryItem(page, next = if (start > 0) page.firstOrNull()?.messageId else null)
        }
        is ChatPageAnchor.After -> {
            val start = anchor.cursor?.let(::indexOf)?.plus(1) ?: 0
            val end = minOf(start + size, messages.size)
            val page = if (start < end) messages.subList(start, end).toList() else emptyList()
            ProductChatMessagesQueryItem(page, next = if (end < messages.size) page.lastOrNull()?.messageId else null)
        }
    }

    /** The messages whose text holds [text], newest first. */
    fun search(text: String, cursor: String?, size: Int): ProductChatSearchQueryItem {
        val needle = text.trim()
        if (needle.isEmpty()) return ProductChatSearchQueryItem(emptyList(), next = null)

        val hits = messages.reversed().filter { it.body.searchText()?.contains(needle, ignoreCase = true) == true }
        val start = cursor?.let { cursor -> hits.indexOfFirst { it.messageId == cursor } + 1 } ?: 0
        val end = minOf(start + size, hits.size)
        val page = if (start < end) hits.subList(start, end) else emptyList()
        return ProductChatSearchQueryItem(page, next = if (end < hits.size) page.lastOrNull()?.messageId else null)
    }

    fun append(message: ChatMessage) {
        messages += message
    }

    /** False for a message that the room does not hold. */
    fun edit(messageId: String, body: ChatMessageBody, editedAt: ULong): Boolean = update(messageId) {
        it.copy(body = body, editedAt = editedAt)
    }

    fun setStatus(messageId: String, status: ChatMessageStatus): Boolean = update(messageId) { it.copy(status = status) }

    fun remove(messageId: String): Boolean = messages.removeAll { it.messageId == messageId }

    /** Places or removes one reaction. [byUser] marks the reaction of the user of the host. */
    fun react(messageId: String, emoji: String, placed: Boolean, byUser: Boolean): Boolean = update(messageId) { message ->
        val summaries = message.reactions.toMutableList()
        val index = summaries.indexOfFirst { it.emoji == emoji }
        val summary = summaries.getOrNull(index)

        when {
            placed && summary == null -> summaries += ChatReactionSummary(emoji, count = 1u, reactedByUser = byUser)
            placed && summary != null && !(byUser && summary.reactedByUser) ->
                summaries[index] = summary.copy(count = summary.count + 1u, reactedByUser = summary.reactedByUser || byUser)
            !placed && summary != null && (!byUser || summary.reactedByUser) -> {
                val left = summary.copy(count = summary.count - 1u, reactedByUser = summary.reactedByUser && !byUser)
                if (left.count == 0u) summaries.removeAt(index) else summaries[index] = left
            }
        }
        message.copy(reactions = summaries)
    }

    private fun indexOf(messageId: String): Int? = messages.indexOfFirst { it.messageId == messageId }.takeIf { it >= 0 }

    private fun update(messageId: String, change: (ChatMessage) -> ChatMessage): Boolean {
        val index = indexOf(messageId) ?: return false
        messages[index] = change(messages[index])
        return true
    }
}

/** The text of a body, for search and for the answers of the product. */
fun ChatMessageBody.searchText(): String? = when (this) {
    is ChatMessageBody.Text -> text
    is ChatMessageBody.RichText -> text
    is ChatMessageBody.Actions -> v1.text
    is ChatMessageBody.Custom -> previewText
    is ChatMessageBody.Notice -> text
}
