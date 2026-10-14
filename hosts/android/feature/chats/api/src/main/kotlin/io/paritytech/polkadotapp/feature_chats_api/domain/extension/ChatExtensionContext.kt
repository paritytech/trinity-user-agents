package io.paritytech.polkadotapp.feature_chats_api.domain.extension

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import kotlinx.coroutines.flow.Flow
import kotlin.reflect.KClass

interface ChatExtensionContext {
    val scope: ComputationalScope

    suspend fun createRoom(request: CreateRoomRequest): CreateRoomResult
    suspend fun deleteRoom(chatId: ChatId)
    suspend fun updateRoomMetadata(chatId: ChatId, metadata: RoomMetadata)

    suspend fun sendMessage(chatId: ChatId, content: ChatMessage.Content): ChatMessage
    suspend fun modifyMessage(chatId: ChatId, messageId: ChatMessageId, content: ChatMessage.Content)
    suspend fun removeMessage(chatId: ChatId, messageId: ChatMessageId)
    suspend fun getPersistedMessages(chatId: ChatId): List<ChatMessage>

    suspend fun setWelcomeMessages(chatId: ChatId, messageBuilder: () -> List<ChatMessage.Content>)

    fun subscribeNewMessages(
        roomFilter: NewMessagesRoomFilter = NewMessagesRoomFilter.Everything,
        contentTypes: Collection<KClass<out ChatMessage.Content>>? = null,
    ): Flow<ChatMessage>

    suspend fun getUnprocessedMessages(
        roomIds: Collection<ChatId>? = null,
        contentTypes: Collection<KClass<out ChatMessage.Content>>? = null,
    ): List<ChatMessage>

    suspend fun markMessageProcessed(chatId: ChatId, messageId: ChatMessageId)

    fun subscribeOwnRooms(): Flow<List<ChatId>>
}

suspend fun ChatExtensionContext.markMessageProcessed(message: ChatMessage) {
    markMessageProcessed(message.chatId, message.id)
}
