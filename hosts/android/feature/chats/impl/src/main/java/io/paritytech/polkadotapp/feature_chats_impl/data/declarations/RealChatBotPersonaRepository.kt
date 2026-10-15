package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.paritytech.polkadotapp.database.dao.ChatBotPersonaDao
import io.paritytech.polkadotapp.database.model.ChatBotPersonaLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBotPersonaRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject

interface ChatBotPersonaWriter {
    /** Adds the persona, or replaces the name and the icon of a persona with the same id. */
    suspend fun put(extensionId: ChatExtensionId, persona: ChatBotPersonaModel)

    suspend fun exists(extensionId: ChatExtensionId, botId: String): Boolean

    suspend fun deleteAll(extensionId: ChatExtensionId)
}

class RealChatBotPersonaRepository @Inject constructor(
    private val dao: ChatBotPersonaDao,
) : ChatBotPersonaRepository, ChatBotPersonaWriter {
    override fun observe(extensionId: ChatExtensionId): Flow<List<ChatBotPersonaModel>> =
        dao.observe(extensionId).map { rows -> rows.map { it.toDomain() } }.distinctUntilChanged()

    override suspend fun put(extensionId: ChatExtensionId, persona: ChatBotPersonaModel) =
        dao.put(ChatBotPersonaLocal(extensionId, persona.botId, persona.name, persona.icon))

    override suspend fun exists(extensionId: ChatExtensionId, botId: String): Boolean = dao.exists(extensionId, botId)

    override suspend fun deleteAll(extensionId: ChatExtensionId) = dao.deleteAll(extensionId)

    private fun ChatBotPersonaLocal.toDomain() = ChatBotPersonaModel(botId, name, icon)
}
