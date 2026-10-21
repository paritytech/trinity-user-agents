package io.paritytech.polkadotapp.feature_chats_api.domain.extension

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
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

    // Chat Extension v2. Each call below fails with `UnknownChatRoomException` when the extension owns no such room.

    /** What the extension declared for the room. Null when it declared nothing. */
    suspend fun roomDeclarations(chatId: ChatId): ChatRoomDeclarations?

    /**
     * Replaces the configuration of the room and starts a new configuration generation, also for the same
     * configuration. A change of [ChatRoomConfigModel.productSource] discards the stored messages of the room.
     */
    suspend fun setRoomConfig(chatId: ChatId, config: ChatRoomConfigModel): Result<Unit>

    /** Replaces the name, the description and the avatar of the room. The v1 icon of the room stays. */
    suspend fun setRoomMetadata(chatId: ChatId, name: String, metadata: ChatRoomMetadataModel): Result<Unit>

    suspend fun setRoomPreview(chatId: ChatId, preview: ChatRoomPreviewModel): Result<Unit>

    suspend fun setRoomFooter(chatId: ChatId, footer: ChatRoomFooterModel): Result<Unit>

    /**
     * Stores one change that the extension pushes to a room whose messages the host keeps. Message ids are the
     * ids of the extension. A refusal is a `ProductFeedRejection`.
     */
    suspend fun applyFeedUpdate(chatId: ChatId, update: ProductFeedUpdateModel): Result<Unit>

    // Chat Extension v2 additions. Each call below fails with `UnknownChatRoomException` when the extension owns no such room.

    /** Replaces the features of the room whole. The host keeps a feature also when it does not serve it. */
    suspend fun setRoomFeatures(chatId: ChatId, features: ChatRoomFeaturesModel): Result<Unit>

    /** Shows the call of the room, or removes it with null. The host keeps it in memory only. */
    suspend fun setRoomCall(chatId: ChatId, call: ChatRoomCallModel?): Result<Unit>
}

suspend fun ChatExtensionContext.markMessageProcessed(message: ChatMessage) {
    markMessageProcessed(message.chatId, message.id)
}
