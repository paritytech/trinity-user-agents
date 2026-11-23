package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatSeenStateLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ChatSeenDao {
    @Query("SELECT sortOrder FROM chat_messages WHERE chatId = :chatId AND origintype = 'USER' AND id = :messageId")
    abstract suspend fun getOutgoingOrder(chatId: ByteArray, messageId: String): Long?

    @Query(
        """
        SELECT MAX(sortOrder) FROM chat_messages
        WHERE chatId = :chatId AND origintype = 'USER' AND status IN ('IS_SENT', 'IS_DELIVERED') AND timestamp <= :timestamp
        """
    )
    abstract suspend fun getHighestSentOutgoingOrderUpTo(chatId: ByteArray, timestamp: Long): Long?

    @Query("SELECT sortOrder FROM chat_messages WHERE chatId = :chatId AND origintype != 'USER' AND id = :messageId")
    abstract suspend fun getIncomingOrder(chatId: ByteArray, messageId: String): Long?

    @Query(
        """
        INSERT OR REPLACE INTO chat_seen_states
            (contactAccountId, peerSeenOrder, lastSentSeenMessageId, lastSentSeenOrder, lastSentSeenTimestamp)
        SELECT
            :contactAccountId,
            :order,
            (SELECT lastSentSeenMessageId FROM chat_seen_states WHERE contactAccountId = :contactAccountId),
            COALESCE((SELECT lastSentSeenOrder FROM chat_seen_states WHERE contactAccountId = :contactAccountId), 0),
            COALESCE((SELECT lastSentSeenTimestamp FROM chat_seen_states WHERE contactAccountId = :contactAccountId), 0)
        WHERE NOT EXISTS (
            SELECT 1 FROM chat_seen_states
            WHERE contactAccountId = :contactAccountId AND peerSeenOrder >= :order
        )
        """
    )
    abstract suspend fun raisePeerSeenOrder(contactAccountId: ByteArray, order: Long)

    @Query(
        """
        INSERT OR REPLACE INTO chat_seen_states
            (contactAccountId, peerSeenOrder, lastSentSeenMessageId, lastSentSeenOrder, lastSentSeenTimestamp)
        SELECT
            :contactAccountId,
            COALESCE((SELECT peerSeenOrder FROM chat_seen_states WHERE contactAccountId = :contactAccountId), 0),
            :messageId,
            :order,
            :timestamp
        WHERE NOT EXISTS (
            SELECT 1 FROM chat_seen_states
            WHERE contactAccountId = :contactAccountId
            AND (lastSentSeenOrder > :order OR (lastSentSeenOrder = :order AND lastSentSeenTimestamp >= :timestamp))
        )
        """
    )
    abstract suspend fun raiseLastSentSeen(contactAccountId: ByteArray, messageId: String, order: Long, timestamp: Long)

    @Query("SELECT * FROM chat_seen_states WHERE contactAccountId = :contactAccountId")
    abstract suspend fun getSeenState(contactAccountId: ByteArray): ChatSeenStateLocal?

    @Query(
        """
        SELECT id, sortOrder, timestamp FROM chat_messages
        WHERE chatId = :chatId AND origintype != 'USER' AND status = 'IS_READ' AND type IN (:types)
        ORDER BY sortOrder DESC, timestamp DESC
        LIMIT 1
        """
    )
    abstract fun subscribeLastSeenIncomingMessage(chatId: ByteArray, types: List<ChatMessageLocal.Type>): Flow<LastSeenMessageProjection?>

    @Query("SELECT peerSeenOrder FROM chat_seen_states WHERE contactAccountId = :contactAccountId")
    abstract fun subscribePeerSeenOrder(contactAccountId: ByteArray): Flow<Long?>

    data class LastSeenMessageProjection(
        val id: String,
        val sortOrder: Long,
        val timestamp: Long
    )
}
