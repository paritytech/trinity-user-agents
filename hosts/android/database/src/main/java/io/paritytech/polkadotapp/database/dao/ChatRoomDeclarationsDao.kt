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

    @Query("SELECT chatId, mutedUntil, archived FROM chat_room_declarations WHERE mutedUntil IS NOT NULL OR archived = 1")
    abstract fun observeUserStates(): Flow<List<UserState>>

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
    open suspend fun setFooter(chatId: ByteArray, footer: ByteArray) {
        insertEmpty(chatId)
        updateFooter(chatId, footer)
    }

    @Transaction
    open suspend fun setFeatures(chatId: ByteArray, features: ByteArray) {
        insertEmpty(chatId)
        updateFeatures(chatId, features)
    }

    @Transaction
    open suspend fun setUserState(chatId: ByteArray, mutedUntil: Long?, archived: Boolean) {
        insertEmpty(chatId)
        updateUserState(chatId, mutedUntil, archived)
    }

    @Query("INSERT OR IGNORE INTO chat_room_declarations (chatId, configGeneration) VALUES (:chatId, 0)")
    protected abstract suspend fun insertEmpty(chatId: ByteArray)

    @Query("UPDATE chat_room_declarations SET config = :config, configGeneration = configGeneration + 1 WHERE chatId = :chatId")
    protected abstract suspend fun updateConfig(chatId: ByteArray, config: ByteArray)

    @Query("UPDATE chat_room_declarations SET description = :description, avatar = :avatar WHERE chatId = :chatId")
    protected abstract suspend fun updateMetadata(chatId: ByteArray, description: String?, avatar: ByteArray?)

    @Query("UPDATE chat_room_declarations SET preview = :preview WHERE chatId = :chatId")
    protected abstract suspend fun updatePreview(chatId: ByteArray, preview: ByteArray)

    @Query("UPDATE chat_room_declarations SET footer = :footer WHERE chatId = :chatId")
    protected abstract suspend fun updateFooter(chatId: ByteArray, footer: ByteArray)

    @Query("UPDATE chat_room_declarations SET features = :features WHERE chatId = :chatId")
    protected abstract suspend fun updateFeatures(chatId: ByteArray, features: ByteArray)

    @Query("UPDATE chat_room_declarations SET mutedUntil = :mutedUntil, archived = :archived WHERE chatId = :chatId")
    protected abstract suspend fun updateUserState(chatId: ByteArray, mutedUntil: Long?, archived: Boolean)

    class UserState(val chatId: ByteArray, val mutedUntil: Long?, val archived: Boolean)
}
