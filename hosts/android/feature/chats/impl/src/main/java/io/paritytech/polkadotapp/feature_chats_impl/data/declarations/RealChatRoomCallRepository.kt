package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

interface ChatRoomCallWriter {
    /** Shows [call] in the room, or removes it with null. */
    fun set(chatId: ChatId, call: ChatRoomCallModel?)
}

// A call lives only while the worker of its product runs, so the host keeps it in memory, as the activity line.
@Singleton
class RealChatRoomCallRepository @Inject constructor() : ChatRoomCallRepository, ChatRoomCallWriter {
    private val calls = MutableStateFlow<Map<ChatId, ChatRoomCallModel>>(emptyMap())

    override fun observe(chatId: ChatId): Flow<ChatRoomCallModel?> =
        calls.map { it[chatId] }.distinctUntilChanged()

    override fun set(chatId: ChatId, call: ChatRoomCallModel?) {
        calls.update { if (call == null) it - chatId else it + (chatId to call) }
    }
}
