package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.database.dao.ProcessedChatMessageDao
import io.paritytech.polkadotapp.database.model.ProcessedChatMessageLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import kotlinx.coroutines.withContext
import javax.inject.Inject

interface ProcessedChatMessageRepository {
    suspend fun tryMarkProcessed(messageIds: List<ChatMessageId>): Set<ChatMessageId>

    suspend fun tryMarkProcessed(messageId: ChatMessageId): Boolean
}

internal class RealProcessedChatMessageRepository @Inject constructor(
    private val dao: ProcessedChatMessageDao,
    private val coroutineDispatchers: CoroutineDispatchers,
) : ProcessedChatMessageRepository {
    override suspend fun tryMarkProcessed(messageIds: List<ChatMessageId>): Set<ChatMessageId> {
        if (messageIds.isEmpty()) return emptySet()

        return withContext(coroutineDispatchers.io) {
            val rowIds = dao.insertIfNotExists(messageIds.map(::ProcessedChatMessageLocal))
            messageIds.filterIndexed { index, _ -> rowIds[index] >= 0 }.toSet()
        }
    }

    override suspend fun tryMarkProcessed(messageId: ChatMessageId): Boolean {
        return tryMarkProcessed(listOf(messageId)).isNotEmpty()
    }
}
