package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.RemovedChatLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface RemovedChatDao {
    @Query("SELECT * FROM removed_chats WHERE removedAt > :after")
    fun observeRemovedAfter(after: Long): Flow<List<RemovedChatLocal>>

    @Query("SELECT * FROM removed_chats WHERE removedAt > :after")
    suspend fun getRemovedAfter(after: Long): List<RemovedChatLocal>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(removedChat: RemovedChatLocal)

    @Query("DELETE FROM removed_chats WHERE accountId = :accountId")
    suspend fun delete(accountId: ByteArray)
}
