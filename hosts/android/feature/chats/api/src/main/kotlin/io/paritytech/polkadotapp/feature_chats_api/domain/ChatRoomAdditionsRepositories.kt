package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import kotlinx.coroutines.flow.Flow

// The readers of the Chat Extension v2 additions. The room features are part of `ChatRoomDeclarations`.

/** Reads what the user set for the product rooms: mute and archive. */
interface ChatRoomUserStateRepository {
    /** Emits [ChatRoomUserStateModel.NONE] for a room that the user did not mute or archive. */
    fun observe(chatId: ChatId): Flow<ChatRoomUserStateModel>

    /** Contains only the rooms that the user muted or archived. */
    fun observeAll(): Flow<Map<ChatId, ChatRoomUserStateModel>>
}

/** Reads the meta that the products set for the messages of their rooms. */
interface ChatMessageMetaRepository {
    /** The meta of each message of the room, by the message id of the product. */
    fun observe(chatId: ChatId): Flow<Map<String, ChatMessageMetaModel>>
}

/** Reads the call of each room. The host keeps it in memory only. */
interface ChatRoomCallRepository {
    fun observe(chatId: ChatId): Flow<ChatRoomCallModel?>
}

/** Reads the bot personas of the extensions. */
interface ChatBotPersonaRepository {
    fun observe(extensionId: ChatExtensionId): Flow<List<ChatBotPersonaModel>>
}
