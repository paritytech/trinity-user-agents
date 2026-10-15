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

    /** The menu of [message]. Its ids are the ids of the room screen. */
    suspend fun messageMenu(chatId: ChatId, message: ProductRoomMessage): Result<ChatMessageMenuModel> =
        Result.failure(ProductChatQueryException.NotAvailable())

    suspend fun participantsCount(chatId: ChatId): Result<Int> = Result.failure(ProductChatQueryException.NotAvailable())

    /** A null [cursor] asks for the first page. */
    suspend fun participantsPage(chatId: ChatId, cursor: String?, pageSize: Int): Result<ProductParticipantsPage> =
        Result.failure(ProductChatQueryException.NotAvailable())

    /** The participants with these ids that the product knows. An unknown id is not in the answer. */
    suspend fun participantsByIds(chatId: ChatId, participantIds: List<String>): Result<List<ChatParticipantEntryModel>> =
        Result.failure(ProductChatQueryException.NotAvailable())

    /** Who reacted to the message with id [messageId] of the room screen, in the order of the product. */
    suspend fun reactions(chatId: ChatId, messageId: String): Result<List<ChatReactionDetailModel>> =
        Result.failure(ProductChatQueryException.NotAvailable())

    /** The menu of one participant of the room. */
    suspend fun participantMenu(chatId: ChatId, entry: ChatParticipantEntryModel): Result<ChatParticipantMenuModel> =
        Result.failure(ProductChatQueryException.NotAvailable())

    /**
     * The messages of the room that match [text], newest first (`search_query`, 07-LATER §8.2). A null [cursor]
     * asks for the newest matches. The host asks it only inside the open room, never from the chat list (D29).
     */
    suspend fun searchPage(chatId: ChatId, text: String, cursor: String?, pageSize: Int): Result<ProductSearchPage> =
        Result.failure(ProductChatQueryException.NotAvailable())
}

/** One page of the search hits of a product, the newest first. A null [next] ends the older matches. */
data class ProductSearchPage(val hits: List<ProductRoomMessage>, val next: String?)
