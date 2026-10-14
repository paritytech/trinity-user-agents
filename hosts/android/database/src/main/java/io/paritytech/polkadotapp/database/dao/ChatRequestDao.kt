package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ChatRequestLocal
import io.paritytech.polkadotapp.database.model.ChatRequestLocal.Status
import kotlinx.coroutines.flow.Flow

@Dao
interface ChatRequestDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(request: ChatRequestLocal)

    @Query("SELECT * FROM chat_requests WHERE id = :id")
    suspend fun getById(id: String): ChatRequestLocal?

    @Query("SELECT * FROM chat_requests WHERE id = :id")
    fun subscribeById(id: String): Flow<ChatRequestLocal?>

    @Query("UPDATE chat_requests SET status = :status WHERE id = :id")
    suspend fun updateStatus(id: String, status: Status)

    @Query("DELETE FROM chat_requests WHERE id = :id")
    suspend fun delete(id: String)
}
