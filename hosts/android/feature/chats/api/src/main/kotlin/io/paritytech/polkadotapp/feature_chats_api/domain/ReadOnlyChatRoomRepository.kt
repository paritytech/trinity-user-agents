package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlinx.coroutines.flow.Flow

interface ReadOnlyChatRoomRepository {
    suspend fun getRoomMetadata(chatId: ChatId): RoomMetadata

    fun subscribeRoomsByExtension(extensionId: ChatExtensionId): Flow<List<ChatId>>
}
