package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.ChatSearchRecentLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ChatSearchRecentDao {
    @Query("SELECT * FROM chat_search_recents ORDER BY timestamp DESC LIMIT $MAX_RECENTS")
    abstract fun observeRecents(): Flow<List<ChatSearchRecentLocal>>

    @Query("DELETE FROM chat_search_recents WHERE chatId = :chatId")
    abstract suspend fun removeRecent(chatId: ByteArray)

    @Query("DELETE FROM chat_search_recents")
    abstract suspend fun clearAll()

    @Transaction
    open suspend fun upsertRecent(recent: ChatSearchRecentLocal) {
        insert(recent)
        pruneToLimit()
    }

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    protected abstract suspend fun insert(recent: ChatSearchRecentLocal)

    @Query(
        """
        DELETE FROM chat_search_recents
        WHERE chatId NOT IN (
            SELECT chatId FROM chat_search_recents ORDER BY timestamp DESC LIMIT $MAX_RECENTS
        )
        """
    )
    protected abstract suspend fun pruneToLimit()
}

private const val MAX_RECENTS = 20
