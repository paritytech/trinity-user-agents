package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Query
import androidx.room.Upsert
import io.paritytech.polkadotapp.database.model.ChatDraftLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface ChatDraftDao {
    @Upsert
    suspend fun upsert(draft: ChatDraftLocal)

    @Query("DELETE FROM chat_drafts WHERE chatId = :chatId")
    suspend fun deleteByChatId(chatId: ByteArray)

    @Query("SELECT * FROM chat_drafts WHERE chatId = :chatId")
    suspend fun getDraft(chatId: ByteArray): ChatDraftLocal?

    @Query("SELECT * FROM chat_drafts")
    fun subscribeDrafts(): Flow<List<ChatDraftLocal>>
}
