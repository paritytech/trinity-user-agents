package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ChatBotPersonaLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ChatBotPersonaDao {
    @Query("SELECT * FROM chat_bots WHERE extensionId = :extensionId ORDER BY botId")
    abstract fun observe(extensionId: String): Flow<List<ChatBotPersonaLocal>>

    @Query("SELECT EXISTS(SELECT 1 FROM chat_bots WHERE extensionId = :extensionId AND botId = :botId)")
    abstract suspend fun exists(extensionId: String, botId: String): Boolean

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    abstract suspend fun put(persona: ChatBotPersonaLocal)

    @Query("DELETE FROM chat_bots WHERE extensionId = :extensionId")
    abstract suspend fun deleteAll(extensionId: String)
}
