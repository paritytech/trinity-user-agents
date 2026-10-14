package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.database.dao.ChatSearchRecentDao
import io.paritytech.polkadotapp.database.model.ChatSearchRecentLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.RecentChat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

interface ChatSearchRecentsRepository {
    fun observeRecents(): Flow<List<RecentChat>>
    suspend fun addRecent(chatId: ChatId)
    suspend fun removeRecent(chatId: ChatId)
    suspend fun clearRecents()
}

class RealChatSearchRecentsRepository @Inject constructor(
    private val dao: ChatSearchRecentDao,
) : ChatSearchRecentsRepository {
    override fun observeRecents(): Flow<List<RecentChat>> {
        return dao.observeRecents().map { locals ->
            locals.map { it.toDomain() }
        }
    }

    override suspend fun addRecent(chatId: ChatId) {
        dao.upsertRecent(
            ChatSearchRecentLocal(
                chatId = chatId.toLocal(),
                timestamp = System.currentTimeMillis()
            )
        )
    }

    override suspend fun removeRecent(chatId: ChatId) {
        dao.removeRecent(chatId.toLocal())
    }

    override suspend fun clearRecents() {
        dao.clearAll()
    }

    private fun ChatSearchRecentLocal.toDomain(): RecentChat {
        return RecentChat(
            chatId = ChatId.fromRawValue(chatId),
            timestamp = timestamp
        )
    }
}
