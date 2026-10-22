package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import kotlinx.coroutines.flow.Flow

/** Reads the call of each room. The host keeps it in memory only. */
interface ChatRoomCallRepository {
    fun observe(chatId: ChatId): Flow<ChatRoomCallModel?>
}
