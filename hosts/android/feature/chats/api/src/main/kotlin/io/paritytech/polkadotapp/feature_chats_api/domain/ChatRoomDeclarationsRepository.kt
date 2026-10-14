package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import kotlinx.coroutines.flow.Flow

/** Reads what the products declared for their rooms. */
interface ChatRoomDeclarationsRepository {
    /** Emits the declarations of the room at each change. Null when the room has none. */
    fun observe(chatId: ChatId): Flow<ChatRoomDeclarations?>

    suspend fun get(chatId: ChatId): ChatRoomDeclarations?
}

/** Reads the activity line of the rooms. The host keeps it in memory only, and a deleted room loses it. */
interface ChatRoomActivityRepository {
    fun observe(chatId: ChatId): Flow<ChatRoomActivityModel?>
}
