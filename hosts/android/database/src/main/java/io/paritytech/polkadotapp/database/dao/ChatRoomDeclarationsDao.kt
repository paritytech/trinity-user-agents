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

    @Query("SELECT * FROM chat_room_declarations WHERE contact IS NOT NULL")
    abstract fun observeWithContact(): Flow<List<ChatRoomDeclarationsLocal>>

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

    @Transaction
    open suspend fun setFeatures(chatId: ByteArray, features: ByteArray) {
        insertEmpty(chatId)
        updateFeatures(chatId, features)
    }

    @Transaction
    open suspend fun setContact(chatId: ByteArray, contact: ByteArray) {
        insertEmpty(chatId)
        updateContact(chatId, contact)
    }

    @Query("INSERT OR IGNORE INTO chat_room_declarations (chatId, configGeneration) VALUES (:chatId, 0)")
    protected abstract suspend fun insertEmpty(chatId: ByteArray)

    @Query("UPDATE chat_room_declarations SET config = :config, configGeneration = configGeneration + 1 WHERE chatId = :chatId")
    protected abstract suspend fun updateConfig(chatId: ByteArray, config: ByteArray)

    @Query("UPDATE chat_room_declarations SET description = :description WHERE chatId = :chatId")
    protected abstract suspend fun updateMetadata(chatId: ByteArray, description: String?)

    @Query("UPDATE chat_room_declarations SET preview = :preview WHERE chatId = :chatId")
    protected abstract suspend fun updatePreview(chatId: ByteArray, preview: ByteArray)

    @Query("UPDATE chat_room_declarations SET features = :features WHERE chatId = :chatId")
    protected abstract suspend fun updateFeatures(chatId: ByteArray, features: ByteArray)

    @Query("UPDATE chat_room_declarations SET contact = :contact WHERE chatId = :chatId")
    protected abstract suspend fun updateContact(chatId: ByteArray, contact: ByteArray)
}
