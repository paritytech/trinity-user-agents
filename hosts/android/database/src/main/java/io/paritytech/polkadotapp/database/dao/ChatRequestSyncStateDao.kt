package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ChatRequestSyncStateLocal

@Dao
interface ChatRequestSyncStateDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(syncState: ChatRequestSyncStateLocal)

    @Query("SELECT * FROM chat_request_sync_state WHERE metaAccountId = :metaAccountId")
    suspend fun get(metaAccountId: Long): ChatRequestSyncStateLocal?

    @Query("DELETE FROM chat_request_sync_state WHERE metaAccountId = :metaAccountId")
    suspend fun delete(metaAccountId: Long)
}
