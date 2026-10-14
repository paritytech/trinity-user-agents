package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ProcessedChatMessageLocal

@Dao
interface ProcessedChatMessageDao {
    @Insert(onConflict = OnConflictStrategy.IGNORE)
    suspend fun insertIfNotExists(items: List<ProcessedChatMessageLocal>): List<Long>

    @Query("SELECT messageId FROM processed_chat_messages WHERE messageId IN (:messageIds)")
    suspend fun getProcessedIds(messageIds: List<String>): List<String>
}
