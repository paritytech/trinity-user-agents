package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.ChatRoomDeclarationsLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ChatRoomDeclarationsDao {
    @Query("SELECT * FROM chat_room_declarations WHERE chatId = :chatId")
    abstract fun observe(chatId: ByteArray): Flow<ChatRoomDeclarationsLocal?>

    @Query("SELECT * FROM chat_room_declarations WHERE chatId = :chatId")
    abstract suspend fun get(chatId: ByteArray): ChatRoomDeclarationsLocal?

    @Query("DELETE FROM chat_room_declarations WHERE chatId = :chatId")
    abstract suspend fun delete(chatId: ByteArray)

    @Transaction
    open suspend fun setConfig(chatId: ByteArray, config: ByteArray) {
        insertEmpty(chatId)
        updateConfig(chatId, config)
    }

    @Transaction
    open suspend fun setMetadata(chatId: ByteArray, description: String?, avatar: ByteArray?) {
        insertEmpty(chatId)
        updateMetadata(chatId, description, avatar)
    }

    @Transaction
    open suspend fun setPreview(chatId: ByteArray, preview: ByteArray) {
        insertEmpty(chatId)
        updatePreview(chatId, preview)
    }

    @Transaction
    open suspend fun setToolbar(chatId: ByteArray, toolbar: ByteArray) {
        insertEmpty(chatId)
        updateToolbar(chatId, toolbar)
    }

    @Transaction
    open suspend fun setSuggestions(chatId: ByteArray, suggestions: ByteArray) {
        insertEmpty(chatId)
        updateSuggestions(chatId, suggestions)
    }

    @Transaction
    open suspend fun setCommands(chatId: ByteArray, commands: ByteArray) {
        insertEmpty(chatId)
        updateCommands(chatId, commands)
    }

    @Transaction
    open suspend fun setFooter(chatId: ByteArray, footer: ByteArray) {
        insertEmpty(chatId)
        updateFooter(chatId, footer)
    }

    @Query("INSERT OR IGNORE INTO chat_room_declarations (chatId, configGeneration) VALUES (:chatId, 0)")
    protected abstract suspend fun insertEmpty(chatId: ByteArray)

    @Query("UPDATE chat_room_declarations SET config = :config, configGeneration = configGeneration + 1 WHERE chatId = :chatId")
    protected abstract suspend fun updateConfig(chatId: ByteArray, config: ByteArray)

    @Query("UPDATE chat_room_declarations SET description = :description, avatar = :avatar WHERE chatId = :chatId")
    protected abstract suspend fun updateMetadata(chatId: ByteArray, description: String?, avatar: ByteArray?)

    @Query("UPDATE chat_room_declarations SET preview = :preview WHERE chatId = :chatId")
    protected abstract suspend fun updatePreview(chatId: ByteArray, preview: ByteArray)

    @Query("UPDATE chat_room_declarations SET toolbar = :toolbar WHERE chatId = :chatId")
    protected abstract suspend fun updateToolbar(chatId: ByteArray, toolbar: ByteArray)

    @Query("UPDATE chat_room_declarations SET suggestions = :suggestions WHERE chatId = :chatId")
    protected abstract suspend fun updateSuggestions(chatId: ByteArray, suggestions: ByteArray)

    @Query("UPDATE chat_room_declarations SET commands = :commands WHERE chatId = :chatId")
    protected abstract suspend fun updateCommands(chatId: ByteArray, commands: ByteArray)

    @Query("UPDATE chat_room_declarations SET footer = :footer WHERE chatId = :chatId")
    protected abstract suspend fun updateFooter(chatId: ByteArray, footer: ByteArray)
}
