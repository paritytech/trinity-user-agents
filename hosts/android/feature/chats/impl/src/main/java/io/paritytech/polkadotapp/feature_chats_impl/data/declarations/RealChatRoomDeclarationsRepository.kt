package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.paritytech.polkadotapp.database.dao.ChatRoomDeclarationsDao
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.MatchRendererCustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
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

    suspend fun delete(chatId: ChatId)
}

class RealChatRoomDeclarationsRepository @Inject constructor(
    private val dao: ChatRoomDeclarationsDao,
    private val extensionRegistry: ChatExtensionRegistry,
) : ChatRoomDeclarationsRepository, ChatRoomDeclarationsWriter {
    override fun observe(chatId: ChatId): Flow<ChatRoomDeclarations?> =
        dao.observe(chatId.toLocal()).map { it?.toDomain(contentDecoder()) }.distinctUntilChanged()

    override suspend fun get(chatId: ChatId): ChatRoomDeclarations? = dao.get(chatId.toLocal())?.toDomain(contentDecoder())

    override suspend fun setConfig(chatId: ChatId, config: ChatRoomConfigModel) =
        dao.setConfig(chatId.toLocal(), config.encode())

    override suspend fun setMetadata(chatId: ChatId, metadata: ChatRoomMetadataModel) =
        dao.setMetadata(chatId.toLocal(), metadata.description)

    override suspend fun setPreview(chatId: ChatId, preview: ChatRoomPreviewModel) =
        dao.setPreview(chatId.toLocal(), preview.encode(contentDecoder()))

    override suspend fun delete(chatId: ChatId) = dao.delete(chatId.toLocal())

    private suspend fun contentDecoder(): CustomContentDecoder = MatchRendererCustomContentDecoder(
        extensionRegistry.getAllExtensions().flatMap { it.customMessageRenderers() }.associateBy { it.id },
    )
}
