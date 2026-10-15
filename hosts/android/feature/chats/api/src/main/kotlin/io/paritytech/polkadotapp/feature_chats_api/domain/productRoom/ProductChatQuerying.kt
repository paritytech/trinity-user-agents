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
}
