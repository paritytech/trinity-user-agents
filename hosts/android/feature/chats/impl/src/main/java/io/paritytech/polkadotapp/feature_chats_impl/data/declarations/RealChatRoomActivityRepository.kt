package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomActivityRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

interface ChatRoomActivityWriter {
    /** Shows [activity] for the room, or removes the line with null. */
    fun set(chatId: ChatId, activity: ChatRoomActivityModel?)
}

@Singleton
class RealChatRoomActivityRepository @Inject constructor() : ChatRoomActivityRepository, ChatRoomActivityWriter {
    private val activities = MutableStateFlow<Map<ChatId, ChatRoomActivityModel>>(emptyMap())

    override fun observe(chatId: ChatId): Flow<ChatRoomActivityModel?> =
        activities.map { it[chatId] }.distinctUntilChanged()

    override fun set(chatId: ChatId, activity: ChatRoomActivityModel?) {
        activities.update { if (activity == null) it - chatId else it + (chatId to activity) }
    }
}
