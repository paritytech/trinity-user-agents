package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.ChatMessageMetaLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ChatMessageMetaDao {
    @Query("SELECT * FROM chat_message_meta WHERE chatId = :chatId")
    abstract fun observe(chatId: ByteArray): Flow<List<ChatMessageMetaLocal>>

    @Query("SELECT * FROM chat_message_meta")
    abstract fun observeAll(): Flow<List<ChatMessageMetaLocal>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    abstract suspend fun put(meta: ChatMessageMetaLocal)

    /** Writes [meta] and keeps only the [keep] newest rows of its room. */
    @Transaction
    open suspend fun putAndTrim(meta: ChatMessageMetaLocal, keep: Int) {
        put(meta)
        trim(meta.chatId, keep)
    }

    @Query("DELETE FROM chat_message_meta WHERE chatId = :chatId AND messageId = :messageId")
    abstract suspend fun delete(chatId: ByteArray, messageId: String)

    @Query("DELETE FROM chat_message_meta WHERE chatId = :chatId")
    abstract suspend fun deleteRoom(chatId: ByteArray)

    @Query(
        """
        DELETE FROM chat_message_meta WHERE chatId = :chatId AND messageId NOT IN (
            SELECT messageId FROM chat_message_meta WHERE chatId = :chatId
            ORDER BY timestamp DESC, messageId DESC LIMIT :keep
        )
        """
    )
    protected abstract suspend fun trim(chatId: ByteArray, keep: Int)
}
