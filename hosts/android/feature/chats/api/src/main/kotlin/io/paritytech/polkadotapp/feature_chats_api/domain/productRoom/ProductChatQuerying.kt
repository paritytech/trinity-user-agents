package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

/**
 * The queries that a room asks of its product. A failure is a [ProductChatQueryException]. The timeouts are the
 * ones of Chat Extension v2: 10 s for a page, 30 s for a new message, 10 s for the other actions, 5 s for a
 * lookup. A query that the extension does not serve fails with [ProductChatQueryException.NotAvailable].
 */
interface ProductChatQuerying {
    suspend fun messagesPage(chatId: ChatId, anchor: ChatPageAnchorModel, pageSize: Int): Result<ProductMessagesPage>

    suspend fun userAction(chatId: ChatId, action: ChatUserActionModel): Result<ChatUserActionOutcome>

    /** The texts that the message with id [messageId] of the room screen had, oldest first. */
    suspend fun editHistory(chatId: ChatId, messageId: String): Result<List<ChatMessageRevisionModel>> =
        Result.failure(ProductChatQueryException.NotAvailable())

    /** The room to show for a link to [chatId] that carries [payload] (`open_link`); null keeps [chatId]. */
    suspend fun openLink(chatId: ChatId, payload: String): Result<ChatId?> =
        Result.failure(ProductChatQueryException.NotAvailable())

    /**
     * One page of the messages of the room whose text or caption contains [text], the newest first (`search_query`). A
     * null [cursor] starts at the newest.
     */
    suspend fun search(chatId: ChatId, text: String, cursor: String?, pageSize: Int): Result<ProductMessagesPage> =
        Result.failure(ProductChatQueryException.NotAvailable())
}

/** One text that a message had (`ChatMessageRevision`). [editedAt] is in milliseconds since the Unix epoch. */
data class ChatMessageRevisionModel(val editedAt: Long, val text: String?)
