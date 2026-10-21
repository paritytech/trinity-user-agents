package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
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

/** Reads the call of each room. The host keeps it in memory only. */
interface ChatRoomCallRepository {
    fun observe(chatId: ChatId): Flow<ChatRoomCallModel?>
}

/** Reads the bot personas of the extensions. */
interface ChatBotPersonaRepository {
    fun observe(extensionId: ChatExtensionId): Flow<List<ChatBotPersonaModel>>
}

/** The most bot personas that one extension keeps (07-LATER §3.5). */
const val CHAT_BOT_PERSONAS_MAX = 8

/** Keeps the bot personas that the extensions register through `register_bot` (07-LATER §5). */
interface ChatBotPersonaRegistry {
    /**
     * Adds [persona], or gives the persona with the same id the new name and icon. An extension that has
     * [CHAT_BOT_PERSONAS_MAX] personas gets [ChatBotPersonaLimitException] for a new one.
     */
    suspend fun register(extensionId: ChatExtensionId, persona: ChatBotPersonaModel): ChatBotRegistrationModel
}

enum class ChatBotRegistrationModel { NEW, EXISTS }

class ChatBotPersonaLimitException : Exception("a product registers at most $CHAT_BOT_PERSONAS_MAX bots")
