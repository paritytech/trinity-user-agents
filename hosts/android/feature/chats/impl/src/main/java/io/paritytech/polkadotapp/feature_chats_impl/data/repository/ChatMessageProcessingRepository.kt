package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.database.dao.ChatMessageProcessingDao
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatMessageProcessingLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.withContext
import javax.inject.Inject

interface ChatMessageProcessingRepository {
    suspend fun getProcessingHistory(chatId: ChatId): Set<Pair<ChatExtensionId, ChatMessageId>>

    suspend fun flushProcessingHistory(
        chatId: ChatId,
        processed: Set<Pair<ChatExtensionId, ChatMessageId>>
    )

    suspend fun markMessageProcessedByExtension(
        extensionId: ChatExtensionId,
        messageId: ChatMessageId,
        chatId: ChatId,
    )

    suspend fun getUnprocessedMessages(extensionId: ChatExtensionId): List<ChatMessageLocal>

    suspend fun hasBotSentWelcomeMessage(botId: ChatExtensionId): Boolean

    suspend fun markWelcomeMessageSent(botId: ChatExtensionId)

    suspend fun getLastRevealedTimestamp(botId: ChatExtensionId): Timestamp?

    suspend fun setLastRevealedTimestamp(botId: ChatExtensionId, timestamp: Timestamp)
}

internal class RealChatMessageProcessingRepository @Inject constructor(
    private val dao: ChatMessageProcessingDao,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val preferences: Preferences,
) : ChatMessageProcessingRepository {
    companion object {
        private const val WELCOME_MESSAGE_KEY = "RealChatMessageProcessingRepository.WELCOME_MESSAGE_KEY"
        private const val LAST_REVEALED_KEY = "RealChatMessageProcessingRepository.LAST_REVEALED_TIMESTAMP_KEY"
    }

    override suspend fun getProcessingHistory(chatId: ChatId): Set<Pair<ChatExtensionId, ChatMessageId>> {
        return withContext(coroutineDispatchers.io) {
            dao.getMessageProcessingHistory(chatId.toLocal())
                .mapToSet { it.middlewareId to it.messageId }
        }
    }

    override suspend fun flushProcessingHistory(
        chatId: ChatId,
        processed: Set<Pair<ChatExtensionId, ChatMessageId>>
    ) {
        withContext(coroutineDispatchers.io) {
            val local = processed.map {
                ChatMessageProcessingLocal(
                    chatId = chatId.toLocal(),
                    messageId = it.second,
                    middlewareId = it.first
                )
            }

            dao.insertMessageProcessingHistory(local)
        }
    }

    override suspend fun markMessageProcessedByExtension(
        extensionId: ChatExtensionId,
        messageId: ChatMessageId,
        chatId: ChatId,
    ) {
        dao.insertIfNotExists(
            ChatMessageProcessingLocal(
                chatId = chatId.toLocal(),
                messageId = messageId,
                middlewareId = extensionId
            )
        )
    }

    override suspend fun getUnprocessedMessages(extensionId: ChatExtensionId): List<ChatMessageLocal> {
        return dao.getUnprocessedMessages(extensionId)
    }

    override suspend fun hasBotSentWelcomeMessage(botId: ChatExtensionId): Boolean {
        return preferences.contains(welcomeMessageKey(botId))
    }

    override suspend fun markWelcomeMessageSent(botId: ChatExtensionId) {
        preferences.putBoolean(welcomeMessageKey(botId), true)
    }

    override suspend fun getLastRevealedTimestamp(botId: ChatExtensionId): Timestamp? {
        val key = lastRevealedKey(botId)
        return if (preferences.contains(key)) preferences.getLong(key, 0L) else null
    }

    override suspend fun setLastRevealedTimestamp(botId: ChatExtensionId, timestamp: Timestamp) {
        preferences.putLong(lastRevealedKey(botId), timestamp)
    }

    private fun welcomeMessageKey(botId: ChatExtensionId): String {
        return "$WELCOME_MESSAGE_KEY:$botId"
    }

    private fun lastRevealedKey(botId: ChatExtensionId): String {
        return "$LAST_REVEALED_KEY:$botId"
    }
}
