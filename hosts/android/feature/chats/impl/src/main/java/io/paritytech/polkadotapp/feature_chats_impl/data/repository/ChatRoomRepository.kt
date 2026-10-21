package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.database.dao.ChatRoomDao
import io.paritytech.polkadotapp.database.dao.ChatRoomDao.ChatRoomSummaryLocal
import io.paritytech.polkadotapp.database.model.ChatRoomLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomResult
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

interface ChatRoomRepository : ReadOnlyChatRoomRepository {
    suspend fun createRoom(request: CreateRoomRequest): CreateRoomResult

    suspend fun createRoomIfNotExists(chatId: ChatId)

    suspend fun deleteRoom(chatId: ChatId)

    suspend fun updateRoomMetadata(chatId: ChatId, metadata: RoomMetadata)

    fun subscribeChatSummaries(): Flow<List<ChatRoomSummaryLocal>>
}

class RealChatRoomRepository @Inject constructor(
    private val chatRoomDao: ChatRoomDao,
) : ChatRoomRepository {
    override suspend fun createRoom(request: CreateRoomRequest): CreateRoomResult {
        val existing = chatRoomDao.getRoom(request.chatId.toLocal())
        if (existing != null) {
            return CreateRoomResult(request.chatId, CreateRoomStatus.Exists)
        }

        val room = ChatRoomLocal(
            id = request.chatId.toLocal(),
            createdAt = System.currentTimeMillis(),
            name = request.name,
            icon = request.icon
        )
        chatRoomDao.insert(room)
        return CreateRoomResult(request.chatId, CreateRoomStatus.New)
    }

    override suspend fun createRoomIfNotExists(chatId: ChatId) {
        val room = ChatRoomLocal(
            id = chatId.toLocal(),
            createdAt = System.currentTimeMillis(),
            name = null,
            icon = null
        )
        chatRoomDao.insert(room) // IGNORE on conflict
    }

    override suspend fun deleteRoom(chatId: ChatId) {
        chatRoomDao.delete(chatId.toLocal())
    }

    override suspend fun updateRoomMetadata(chatId: ChatId, metadata: RoomMetadata) {
        chatRoomDao.updateMetadata(chatId.toLocal(), metadata.name, metadata.icon)
    }

    override suspend fun getRoomMetadata(chatId: ChatId): RoomMetadata {
        val room = chatRoomDao.getRoom(chatId.toLocal())
        return RoomMetadata(name = room?.name, icon = room?.icon)
    }

    override fun subscribeChatSummaries(): Flow<List<ChatRoomSummaryLocal>> {
        return chatRoomDao.subscribeChatSummaries()
    }

    override fun subscribeRoomsByExtension(extensionId: ChatExtensionId): Flow<List<ChatId>> {
        val prefix = ChatId.extensionPrefixBytes(extensionId).decodeToString()
        return chatRoomDao.subscribeRoomsByPrefix(prefix)
            .mapList { ChatId.fromRawValue(it.id) }
    }
}
