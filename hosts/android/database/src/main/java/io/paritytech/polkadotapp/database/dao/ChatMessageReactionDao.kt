package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Delete
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ChatMessageReactionLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface ChatMessageReactionDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertReaction(reactionLocal: ChatMessageReactionLocal)

    @Delete
    suspend fun deleteReaction(reactionLocal: ChatMessageReactionLocal)

    @Query("SELECT * FROM chat_message_reactions WHERE chatId = :chatId")
    fun subscribeReactions(chatId: ByteArray): Flow<List<ChatMessageReactionLocal>>
}
