package io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.bot

import io.paritytech.polkadotapp.database.dao.ChatBotStateDao
import io.paritytech.polkadotapp.database.model.ChatBotStateLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotData
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotStateController
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class RealChatBotStateController @Inject constructor(
    private val dao: ChatBotStateDao,
    private val chatMessageRepository: ChatMessageRepository
) : ChatBotStateController {
    override suspend fun isActive(botId: ChatExtensionId): Boolean {
        return dao.get(botId)?.isActive ?: false
    }

    override suspend fun awaitActive(botId: ChatExtensionId) {
        dao.subscribe(botId).first { it?.isActive == true }
    }

    override suspend fun setActive(botId: ChatExtensionId) {
        chatMessageRepository.shiftChatMessagesToTimestamp(ChatId.fromChatBotId(botId), System.currentTimeMillis())
        dao.insert(ChatBotStateLocal(botId, true))
    }

    override fun subscribeActiveBotIds(): Flow<Set<ChatExtensionId>> {
        return dao.subscribeActive()
            .map { activeList -> activeList.toSet() }
    }

    override suspend fun applyDefaultBotStates() {
        val activated = ChatBotData.defaultBots().map { ChatBotStateLocal(middlewareId = it.id, isActive = true) }
        val deactivated = ChatBotData.disabledBots().map { ChatBotStateLocal(middlewareId = it.id, isActive = false) }

        dao.insertAll(activated + deactivated)
    }
}
