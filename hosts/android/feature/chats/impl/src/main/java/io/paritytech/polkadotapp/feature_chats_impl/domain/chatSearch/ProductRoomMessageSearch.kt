package io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatMessageSearchHit
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toSearchableContent
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import javax.inject.Inject

/**
 * Finds messages in the person chats of the chat product, whose messages the product keeps, as the native chat finds
 * them in its database: the messages whose text or caption contains the query, the newest first.
 */
class ProductRoomMessageSearch @Inject constructor(
    private val chatExtensionRegistry: ChatExtensionRegistry,
    private val declarationsRepository: ChatRoomDeclarationsRepository,
) {
    /** The person chats of the chat product among [chatIds]. */
    suspend fun productRooms(chatIds: List<ChatId>): List<ChatId> = chatIds.filter { declarationsRepository.get(it)?.contact != null }

    /** One page of each room, at most [limit] hits in each. A room whose product does not answer finds nothing. */
    suspend fun search(query: String, chatIds: List<ChatId>, limit: Int): List<ChatMessageSearchHit> = coroutineScope {
        chatIds.map { chatId ->
            async {
                val querying = chatExtensionRegistry.getExtensionForChat(chatId)?.roomQuerying(chatId) ?: return@async emptyList()
                querying.search(chatId, query, cursor = null, pageSize = limit)
                    .logFailure("ChatSearch.productMessages")
                    .getOrNull()?.messages.orEmpty()
                    .map { message ->
                        ChatMessageSearchHit(
                            chatId = chatId,
                            messageId = message.messageId,
                            snippet = message.content.toSearchableContent(),
                            timestamp = message.timestamp,
                        )
                    }
            }
        }.awaitAll().flatten()
    }
}
