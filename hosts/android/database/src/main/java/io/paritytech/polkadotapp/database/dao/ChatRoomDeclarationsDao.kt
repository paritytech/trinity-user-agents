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
    open suspend fun setMetadata(chatId: ByteArray, description: String?) {
        insertEmpty(chatId)
        updateMetadata(chatId, description)
    }

    @Transaction
    open suspend fun setPreview(chatId: ByteArray, preview: ByteArray) {
        insertEmpty(chatId)
        updatePreview(chatId, preview)
    }

    @Query("INSERT OR IGNORE INTO chat_room_declarations (chatId, configGeneration) VALUES (:chatId, 0)")
    protected abstract suspend fun insertEmpty(chatId: ByteArray)

    @Query("UPDATE chat_room_declarations SET config = :config, configGeneration = configGeneration + 1 WHERE chatId = :chatId")
    protected abstract suspend fun updateConfig(chatId: ByteArray, config: ByteArray)

    @Query("UPDATE chat_room_declarations SET description = :description WHERE chatId = :chatId")
    protected abstract suspend fun updateMetadata(chatId: ByteArray, description: String?)

    @Query("UPDATE chat_room_declarations SET preview = :preview WHERE chatId = :chatId")
    protected abstract suspend fun updatePreview(chatId: ByteArray, preview: ByteArray)
}
