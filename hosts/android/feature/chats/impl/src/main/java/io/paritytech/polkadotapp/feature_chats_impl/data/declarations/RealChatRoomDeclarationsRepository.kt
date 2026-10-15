package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.paritytech.polkadotapp.database.dao.ChatRoomDeclarationsDao
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
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

    suspend fun setToolbar(chatId: ChatId, actions: List<ChatToolbarActionModel>)

    suspend fun setSuggestions(chatId: ChatId, suggestions: List<ChatSuggestionModel>)

    suspend fun setCommands(chatId: ChatId, commands: List<ChatCommandModel>)

    suspend fun setFooter(chatId: ChatId, footer: ChatRoomFooterModel)

    suspend fun setFeatures(chatId: ChatId, features: ChatRoomFeaturesModel)

    suspend fun delete(chatId: ChatId)
}

/** Reads which rooms are the thread of a message of another room. The chat list does not show them. */
fun interface ChatThreadRooms {
    fun observeThreadRoomIds(): Flow<Set<ChatId>>
}

/** Finds the thread rooms of a room for its deletion (07-LATER §9.2). */
interface ChatThreadRoomCascade {
    /** The rooms of the same extension whose features name a message of [parent] in `threadOf`. */
    suspend fun threadRoomsOf(parent: ChatId): List<ChatId>

    /** Runs [action] in one database transaction. */
    suspend fun inTransaction(action: suspend () -> Unit)
}

class RealChatRoomDeclarationsRepository @Inject constructor(
    private val dao: ChatRoomDeclarationsDao,
) : ChatRoomDeclarationsRepository, ChatRoomDeclarationsWriter, ChatThreadRooms, ChatThreadRoomCascade {
    override fun observe(chatId: ChatId): Flow<ChatRoomDeclarations?> =
        dao.observe(chatId.toLocal()).map { it?.toDomain() }.distinctUntilChanged()

    override suspend fun get(chatId: ChatId): ChatRoomDeclarations? = dao.get(chatId.toLocal())?.toDomain()

    override suspend fun setConfig(chatId: ChatId, config: ChatRoomConfigModel) =
        dao.setConfig(chatId.toLocal(), config.encode())

    override suspend fun setMetadata(chatId: ChatId, metadata: ChatRoomMetadataModel) =
        dao.setMetadata(chatId.toLocal(), metadata.description, metadata.encodeAvatar())

    override suspend fun setPreview(chatId: ChatId, preview: ChatRoomPreviewModel) =
        dao.setPreview(chatId.toLocal(), preview.encode())

    override suspend fun setToolbar(chatId: ChatId, actions: List<ChatToolbarActionModel>) =
        dao.setToolbar(chatId.toLocal(), actions.encodeToolbar())

    override suspend fun setSuggestions(chatId: ChatId, suggestions: List<ChatSuggestionModel>) =
        dao.setSuggestions(chatId.toLocal(), suggestions.encodeSuggestions())

    override suspend fun setCommands(chatId: ChatId, commands: List<ChatCommandModel>) =
        dao.setCommands(chatId.toLocal(), commands.encodeCommands())

    override suspend fun setFooter(chatId: ChatId, footer: ChatRoomFooterModel) =
        dao.setFooter(chatId.toLocal(), footer.encode())

    override suspend fun setFeatures(chatId: ChatId, features: ChatRoomFeaturesModel) =
        dao.setFeatures(chatId.toLocal(), features.encode())

    override suspend fun delete(chatId: ChatId) = dao.delete(chatId.toLocal())

    override suspend fun threadRoomsOf(parent: ChatId): List<ChatId> {
        val room = parent.extensionOrNull() ?: return emptyList()
        val roomId = room.subRoomId ?: return emptyList()
        return dao.getDeclaredFeatures().mapNotNull { row ->
            val chatId = ChatId.fromRawValue(row.chatId)
            val threadOf = row.features.decodeFeatures()?.threadOf ?: return@mapNotNull null
            chatId.takeIf { threadOf.roomId == roomId && chatId.extensionOrNull()?.extensionId == room.extensionId && chatId != parent }
        }
    }

    override suspend fun inTransaction(action: suspend () -> Unit) = dao.withTransaction(action)

    override fun observeThreadRoomIds(): Flow<Set<ChatId>> =
        dao.observeDeclaredFeatures()
            .map { rows ->
                rows.filter { it.features.decodeFeatures()?.threadOf != null }
                    .mapTo(mutableSetOf()) { ChatId.fromRawValue(it.chatId) }
            }
            .distinctUntilChanged()
}
