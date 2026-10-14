package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatMessageProcessingLocal

@Dao
interface ChatMessageProcessingDao {
    @Query("SELECT * FROM chat_message_processing WHERE chatId = :chatId")
    suspend fun getMessageProcessingHistory(chatId: ByteArray): List<ChatMessageProcessingLocal>

    @Insert
    suspend fun insertMessageProcessingHistory(items: List<ChatMessageProcessingLocal>)

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    suspend fun insertIfNotExists(item: ChatMessageProcessingLocal)

    /**
     * Get messages not yet processed by a specific extension.
     * Returns message IDs that do NOT have an entry in chat_message_processing for the given extensionId.
     */
    @Query(
        """
        SELECT m.* FROM chat_messages m
        LEFT JOIN chat_message_processing p ON m.id = p.messageId AND p.middlewareId = :extensionId
        WHERE p.messageId IS NULL AND m.isInternal = 0
        ORDER BY m.timestamp ASC
        """
    )
    suspend fun getUnprocessedMessages(extensionId: String): List<ChatMessageLocal>
}
