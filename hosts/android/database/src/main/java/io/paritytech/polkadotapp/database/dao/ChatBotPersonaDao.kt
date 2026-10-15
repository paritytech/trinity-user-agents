package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.ChatBotPersonaLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ChatBotPersonaDao {
    @Query("SELECT * FROM chat_bots WHERE extensionId = :extensionId ORDER BY botId")
    abstract fun observe(extensionId: String): Flow<List<ChatBotPersonaLocal>>

    @Query("SELECT EXISTS(SELECT 1 FROM chat_bots WHERE extensionId = :extensionId AND botId = :botId)")
    abstract suspend fun exists(extensionId: String, botId: String): Boolean

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    abstract suspend fun put(persona: ChatBotPersonaLocal)

    @Query("DELETE FROM chat_bots WHERE extensionId = :extensionId")
    abstract suspend fun deleteAll(extensionId: String)

    @Query("SELECT COUNT(*) FROM chat_bots WHERE extensionId = :extensionId")
    abstract suspend fun count(extensionId: String): Int

    /**
     * Writes [persona] in one transaction. A new persona needs an extension with fewer than [max] personas. A
     * persona with the same id gets the new name and icon.
     */
    @Transaction
    open suspend fun register(persona: ChatBotPersonaLocal, max: Int): ChatBotRegistrationLocal {
        val existed = exists(persona.extensionId, persona.botId)
        if (!existed && count(persona.extensionId) >= max) return ChatBotRegistrationLocal.FULL

        put(persona)
        return if (existed) ChatBotRegistrationLocal.EXISTS else ChatBotRegistrationLocal.NEW
    }
}

/** The result of [ChatBotPersonaDao.register]. */
enum class ChatBotRegistrationLocal {
    /** The extension did not have the persona. */
    NEW,

    /** The extension had the persona, and the name and the icon are now the new ones. */
    EXISTS,

    /** The extension has the most personas already, so the dao wrote nothing. */
    FULL,
}
