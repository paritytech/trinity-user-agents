package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.database.dao.ChatSeenDao
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.LastSeenMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocalType
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import kotlin.reflect.KClass

interface ChatSeenRepository {
    suspend fun getOutgoingOrder(chatId: ChatId, messageId: ChatMessageId): Long?

    suspend fun getHighestSentOutgoingOrderUpTo(chatId: ChatId, timestamp: Long): Long?

    suspend fun getIncomingOrder(chatId: ChatId, messageId: ChatMessageId): Long?

    suspend fun raisePeerSeenOrder(chatId: ChatId, order: Long)

    suspend fun raiseLastSentSeen(chatId: ChatId, lastSentSeen: LastSeenMessage)

    suspend fun getLastSentSeen(chatId: ChatId): LastSeenMessage?

    fun subscribeLastSeenIncomingMessage(
        chatId: ChatId,
        contentTypes: List<KClass<out ChatMessage.Content>>
    ): Flow<LastSeenMessage?>

    fun subscribePeerSeenOrder(chatId: ChatId): Flow<Long>
}

class RealChatSeenRepository @Inject constructor(
    private val dao: ChatSeenDao,
) : ChatSeenRepository {
    override suspend fun getOutgoingOrder(chatId: ChatId, messageId: ChatMessageId): Long? {
        return dao.getOutgoingOrder(chatId.toLocal(), messageId)
    }

    override suspend fun getHighestSentOutgoingOrderUpTo(chatId: ChatId, timestamp: Long): Long? {
        return dao.getHighestSentOutgoingOrderUpTo(chatId.toLocal(), timestamp)
    }

    override suspend fun getIncomingOrder(chatId: ChatId, messageId: ChatMessageId): Long? {
        return dao.getIncomingOrder(chatId.toLocal(), messageId)
    }

    override suspend fun raisePeerSeenOrder(chatId: ChatId, order: Long) {
        dao.raisePeerSeenOrder(chatId.toLocal(), order)
    }

    override suspend fun raiseLastSentSeen(chatId: ChatId, lastSentSeen: LastSeenMessage) {
        dao.raiseLastSentSeen(chatId.toLocal(), lastSentSeen.id, lastSentSeen.sortOrder, lastSentSeen.timestamp)
    }

    override suspend fun getLastSentSeen(chatId: ChatId): LastSeenMessage? {
        val state = dao.getSeenState(chatId.toLocal()) ?: return null
        val messageId = state.lastSentSeenMessageId ?: return null

        return LastSeenMessage(messageId, state.lastSentSeenOrder, state.lastSentSeenTimestamp)
    }

    override fun subscribeLastSeenIncomingMessage(
        chatId: ChatId,
        contentTypes: List<KClass<out ChatMessage.Content>>
    ): Flow<LastSeenMessage?> {
        return dao.subscribeLastSeenIncomingMessage(chatId.toLocal(), contentTypes.map { it.toLocalType() })
            .map { projection -> projection?.let { LastSeenMessage(it.id, it.sortOrder, it.timestamp) } }
    }

    override fun subscribePeerSeenOrder(chatId: ChatId): Flow<Long> {
        return dao.subscribePeerSeenOrder(chatId.toLocal()).map { it ?: 0 }
    }
}
