package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.paritytech.polkadotapp.database.dao.ChatRoomDeclarationsDao
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/** Writes what the products declare for their rooms. The chat engine is the only writer. */
interface ChatRoomDeclarationsWriter {
    suspend fun setConfig(chatId: ChatId, config: ChatRoomConfigModel)

    suspend fun setMetadata(chatId: ChatId, metadata: ChatRoomMetadataModel)

    suspend fun setPreview(chatId: ChatId, preview: ChatRoomPreviewModel)

    suspend fun setFooter(chatId: ChatId, footer: ChatRoomFooterModel)

    suspend fun setFeatures(chatId: ChatId, features: ChatRoomFeaturesModel)

    suspend fun delete(chatId: ChatId)
}

class RealChatRoomDeclarationsRepository @Inject constructor(
    private val dao: ChatRoomDeclarationsDao,
) : ChatRoomDeclarationsRepository, ChatRoomDeclarationsWriter {
    override fun observe(chatId: ChatId): Flow<ChatRoomDeclarations?> =
        dao.observe(chatId.toLocal()).map { it?.toDomain() }.distinctUntilChanged()

    override suspend fun get(chatId: ChatId): ChatRoomDeclarations? = dao.get(chatId.toLocal())?.toDomain()

    override suspend fun setConfig(chatId: ChatId, config: ChatRoomConfigModel) =
        dao.setConfig(chatId.toLocal(), config.encode())

    override suspend fun setMetadata(chatId: ChatId, metadata: ChatRoomMetadataModel) =
        dao.setMetadata(chatId.toLocal(), metadata.description, metadata.encodeAvatar())

    override suspend fun setPreview(chatId: ChatId, preview: ChatRoomPreviewModel) =
        dao.setPreview(chatId.toLocal(), preview.encode())

    override suspend fun setFooter(chatId: ChatId, footer: ChatRoomFooterModel) =
        dao.setFooter(chatId.toLocal(), footer.encode())

    override suspend fun setFeatures(chatId: ChatId, features: ChatRoomFeaturesModel) =
        dao.setFeatures(chatId.toLocal(), features.encode())

    override suspend fun delete(chatId: ChatId) = dao.delete(chatId.toLocal())
}
