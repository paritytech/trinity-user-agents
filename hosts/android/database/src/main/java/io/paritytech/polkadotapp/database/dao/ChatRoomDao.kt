package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Embedded
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatRoomLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ChatRoomDao {
    @Insert(onConflict = OnConflictStrategy.IGNORE)
    abstract suspend fun insert(room: ChatRoomLocal)

    @Query("DELETE FROM chat_rooms WHERE id = :chatId")
    abstract suspend fun delete(chatId: ByteArray)

    @Query("SELECT * FROM chat_rooms WHERE id = :chatId")
    abstract suspend fun getRoom(chatId: ByteArray): ChatRoomLocal?

    @Query("SELECT * FROM chat_rooms")
    abstract fun subscribeAllRooms(): Flow<List<ChatRoomLocal>>

    @Query("UPDATE chat_rooms SET name = :name, icon = :icon WHERE id = :chatId")
    abstract suspend fun updateMetadata(chatId: ByteArray, name: String?, icon: String?)

    @Query("SELECT * FROM chat_rooms WHERE CAST(id AS TEXT) LIKE :prefix || '%'")
    abstract suspend fun getRoomsByPrefix(prefix: String): List<ChatRoomLocal>

    @Query("SELECT * FROM chat_rooms WHERE CAST(id AS TEXT) LIKE :prefix || '%'")
    abstract fun subscribeRoomsByPrefix(prefix: String): Flow<List<ChatRoomLocal>>

    @Query(
        """
        SELECT r.id AS chatId, r.createdAt, r.name AS roomName, r.icon AS roomIcon,
               COALESCE(counts.unseenCount, 0) AS unseenCount,
               COALESCE(counts.hasUnseenReaction, 0) AS hasUnseenReaction,
               m.id AS msg_id, m.chatId AS msg_chatId, m.timestamp AS msg_timestamp,
               m.updatedAt AS msg_updatedAt, m.sortOrder AS msg_sortOrder,
               m.origintype AS msg_origintype, m.originkey AS msg_originkey,
               m.status AS msg_status, COALESCE(rev.type, m.type) AS msg_type,
               m.searchableContent AS msg_searchableContent,
               COALESCE(rev.content, m.content) AS msg_content,
               m.replyToMessageId AS msg_replyToMessageId, m.isInternal AS msg_isInternal
        FROM chat_rooms r
        LEFT JOIN (
            SELECT chatId,
                   COUNT(CASE WHEN status = 'NEW' AND origintype != 'USER' THEN 1 END) AS unseenCount,
                   MAX(CASE WHEN type IN ('REACTED', 'REACTION_REMOVED') AND status = 'NEW' AND origintype != 'USER' THEN 1 ELSE 0 END) AS hasUnseenReaction
            FROM chat_messages WHERE isInternal = 0
            GROUP BY chatId
        ) counts ON counts.chatId = r.id
        LEFT JOIN chat_messages m ON m.id = (
            SELECT lm.id FROM chat_messages lm
            WHERE lm.chatId = r.id AND lm.isInternal = 0 AND lm.type != 'EDITED'
            ORDER BY lm.sortOrder DESC, lm.timestamp DESC
            LIMIT 1
        )
        LEFT JOIN message_revisions rev ON rev.messageId = m.id AND rev.timestamp = (
            SELECT MAX(r2.timestamp) FROM message_revisions r2 WHERE r2.messageId = m.id
        )
        ORDER BY m.sortOrder DESC, COALESCE(m.timestamp, r.createdAt) DESC
        """
    )
    abstract fun subscribeChatSummaries(): Flow<List<ChatRoomSummaryLocal>>

    data class ChatRoomSummaryLocal(
        val chatId: ByteArray,
        val createdAt: Long,
        val roomName: String?,
        val roomIcon: String?,
        val unseenCount: Int,
        val hasUnseenReaction: Boolean,
        @Embedded(prefix = "msg_") val lastMessage: ChatMessageLocal?,
    )
}
