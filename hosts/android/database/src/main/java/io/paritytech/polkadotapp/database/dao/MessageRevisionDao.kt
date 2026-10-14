package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.MessageRevisionLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface MessageRevisionDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(revision: MessageRevisionLocal)

    @Query(
        """
        SELECT r.*
        FROM message_revisions AS r
        WHERE r.chatId = :chatId
          AND r.timestamp = (
            SELECT MAX(r2.timestamp)
            FROM message_revisions AS r2
            WHERE r2.messageId = r.messageId
              AND r2.chatId = :chatId
          )
        """
    )
    fun subscribeLatestRevisions(chatId: ByteArray): Flow<List<MessageRevisionLocal>>

    @Query("SELECT * FROM message_revisions WHERE messageId = :messageId ORDER BY timestamp DESC LIMIT 1")
    suspend fun getLatestRevision(messageId: String): MessageRevisionLocal?

    @Query("SELECT * FROM message_revisions WHERE messageId = :messageId ORDER BY timestamp DESC")
    suspend fun getRevisionsForMessage(messageId: String): List<MessageRevisionLocal>
}
