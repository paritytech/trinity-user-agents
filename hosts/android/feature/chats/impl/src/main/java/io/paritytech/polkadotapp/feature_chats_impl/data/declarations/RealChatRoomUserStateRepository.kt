package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.paritytech.polkadotapp.database.dao.ChatRoomDeclarationsDao
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/** Writes what the user sets for a product room. Only the user sets it, never the product. */
interface ChatRoomUserStateWriter {
    suspend fun set(chatId: ChatId, state: ChatRoomUserStateModel)
}

// The state is on the declarations row of the room, so it goes with the room when the room is deleted.
class RealChatRoomUserStateRepository @Inject constructor(
    private val dao: ChatRoomDeclarationsDao,
) : ChatRoomUserStateRepository, ChatRoomUserStateWriter {
    override fun observe(chatId: ChatId): Flow<ChatRoomUserStateModel> =
        dao.observe(chatId.toLocal())
            .map { row -> row?.let { ChatRoomUserStateModel(it.mutedUntil, it.archived) } ?: ChatRoomUserStateModel.NONE }
            .distinctUntilChanged()

    override fun observeAll(): Flow<Map<ChatId, ChatRoomUserStateModel>> =
        dao.observeUserStates()
            .map { rows -> rows.associate { ChatId.fromRawValue(it.chatId) to ChatRoomUserStateModel(it.mutedUntil, it.archived) } }
            .distinctUntilChanged()

    override suspend fun set(chatId: ChatId, state: ChatRoomUserStateModel) =
        dao.setUserState(chatId.toLocal(), state.mutedUntil, state.archived)
}
