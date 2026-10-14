package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy.Companion.REPLACE
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ChatBotStateLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface ChatBotStateDao {
    @Query("SELECT * FROM chat_bot_state WHERE middlewareId = :middlewareId")
    suspend fun get(middlewareId: String): ChatBotStateLocal?

    @Query("SELECT * FROM chat_bot_state WHERE middlewareId = :middlewareId")
    fun subscribe(middlewareId: String): Flow<ChatBotStateLocal?>

    @Query("SELECT middlewareId FROM chat_bot_state WHERE isActive = 1")
    fun subscribeActive(): Flow<List<String>>

    @Insert(onConflict = REPLACE)
    suspend fun insert(config: ChatBotStateLocal)

    @Insert(onConflict = REPLACE)
    suspend fun insertAll(configs: List<ChatBotStateLocal>)
}
