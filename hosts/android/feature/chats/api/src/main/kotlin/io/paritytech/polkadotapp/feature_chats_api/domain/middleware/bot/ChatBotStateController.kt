package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import kotlinx.coroutines.flow.Flow

interface ChatBotStateController {
    suspend fun isActive(botId: ChatExtensionId): Boolean

    suspend fun awaitActive(botId: ChatExtensionId)

    suspend fun setActive(botId: ChatExtensionId)

    fun subscribeActiveBotIds(): Flow<Set<ChatExtensionId>>

    suspend fun applyDefaultBotStates()
}
