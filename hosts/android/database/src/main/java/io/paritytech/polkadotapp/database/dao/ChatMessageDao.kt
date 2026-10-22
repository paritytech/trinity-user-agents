package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Update
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatMessagePendingExpansionLocal
import io.paritytech.polkadotapp.database.model.ChatMessageSearchRow
import io.paritytech.polkadotapp.database.model.TransferRetryStateLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ChatMessageDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    protected abstract suspend fun insert(local: List<ChatMessageLocal>)

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    protected abstract suspend fun insertIfNotExists(local: List<ChatMessageLocal>): List<Long>

    /**
     * [onSaved] runs inside the same transaction as the insert, so a caller can make a fact durable exactly
     * when the message is — neither can be rolled back without the other.
     */
    @Transaction
    open suspend fun saveMessage(local: ChatMessageLocal, placement: Placement, onSaved: suspend () -> Unit) {
        saveMessages(listOf(local), placement)
        onSaved()
    }

    @Transaction
    open suspend fun saveMessageIfNotExists(local: ChatMessageLocal, placement: Placement): Long {
        return saveMessagesIfNotExist(listOf(local), placement).single()
    }

    @Transaction
    open suspend fun saveMessages(locals: List<ChatMessageLocal>, placement: Placement) {
        insert(locals.withSortOrders(placement))
        insertPendingExpansions(locals.pendingExpansions())
    }

    @Transaction
    open suspend fun saveMessagesIfNotExist(locals: List<ChatMessageLocal>, placement: Placement): List<Long> {
        val rowIds = insertIfNotExists(locals.withSortOrders(placement))
        val inserted = locals.filterIndexed { index, _ -> rowIds[index] >= 0 }
        insertPendingExpansions(inserted.pendingExpansions())
        return rowIds
    }

    private suspend fun List<ChatMessageLocal>.withSortOrders(placement: Placement): List<ChatMessageLocal> {
        val stored = getSortOrders(map { it.id })
        val allocated = filterNot { it.id in stored }
            .sortedBy { it.timestamp }
            .allocateSortOrders(placement)

        return map { it.withSortOrder(stored[it.id] ?: allocated.getValue(it.id)) }
    }

    private suspend fun List<ChatMessageLocal>.allocateSortOrders(placement: Placement): Map<String, Long> {
        return when (placement) {
            Placement.Latest -> allocateLatest()
            Placement.ByTimestamp -> allocateByTimestamp()
            is Placement.SameAs -> {
                val sortOrder = getSortOrder(placement.messageId) ?: return allocateLatest()
                associate { it.id to sortOrder }
            }
        }
    }

    private suspend fun List<ChatMessageLocal>.allocateLatest(): Map<String, Long> {
        if (isEmpty()) return emptyMap()

        val highest = getHighestSortOrder() ?: ChatMessageLocal.UNORDERED
        return mapIndexed { index, local -> local.id to highest + index + 1 }.toMap()
    }

    private suspend fun List<ChatMessageLocal>.allocateByTimestamp(): Map<String, Long> {
        val latestTimestamps = distinctBy { it.chatId.contentToString() }
            .associate { it.chatId.contentToString() to getLatestTimestamp(it.chatId) }
        val (history, live) = partition { local ->
            val latestTimestamp = latestTimestamps.getValue(local.chatId.contentToString())
            latestTimestamp != null && local.timestamp < latestTimestamp
        }
        val historyOrders = history.associate { local ->
            local.id to (getHighestSortOrderUpTo(local.chatId, local.timestamp) ?: ChatMessageLocal.UNORDERED)
        }

        return historyOrders + live.allocateLatest()
    }

    private fun ChatMessageLocal.withSortOrder(sortOrder: Long) = ChatMessageLocal(
        id = id,
        chatId = chatId,
        timestamp = timestamp,
        updatedAt = updatedAt,
        sortOrder = sortOrder,
        origin = origin,
        status = status,
        type = type,
        searchableContent = searchableContent,
        content = content,
        replyToMessageId = replyToMessageId,
        isInternal = isInternal
    )

    private suspend fun getSortOrders(messageIds: List<String>): Map<String, Long> {
        return messageIds.chunked(SQLITE_VARIABLE_LIMIT)
            .flatMap { getSortOrderRows(it) }
            .associate { it.id to it.sortOrder }
    }

    @Query("SELECT sortOrder FROM chat_messages WHERE id = :messageId")
    protected abstract suspend fun getSortOrder(messageId: String): Long?

    @Query("SELECT id, sortOrder FROM chat_messages WHERE id IN (:messageIds)")
    protected abstract suspend fun getSortOrderRows(messageIds: List<String>): List<SortOrderProjection>

    @Query("SELECT MAX(sortOrder) FROM chat_messages")
    protected abstract suspend fun getHighestSortOrder(): Long?

    @Query("SELECT MAX(sortOrder) FROM chat_messages WHERE chatId = :chatId AND timestamp <= :timestamp")
    protected abstract suspend fun getHighestSortOrderUpTo(chatId: ByteArray, timestamp: Long): Long?

    private fun List<ChatMessageLocal>.pendingExpansions(): List<ChatMessagePendingExpansionLocal> {
        return filter { it.type == ChatMessageLocal.Type.COMPACTION_COMMIT && it.origin.type != ChatMessageLocal.OriginType.USER }
            .map { ChatMessagePendingExpansionLocal(commitId = it.id, retryState = TransferRetryStateLocal.None) }
    }

    @Query("SELECT * FROM chat_messages WHERE chatId = :chatId ORDER BY sortOrder DESC, timestamp DESC")
    abstract fun subscribeMessages(chatId: ByteArray): Flow<List<ChatMessageLocal>>

    @Query("SELECT * FROM chat_messages WHERE chatId = :chatId ORDER BY sortOrder ASC, timestamp ASC")
    abstract suspend fun getMessages(chatId: ByteArray): List<ChatMessageLocal>

    @Query("SELECT * FROM chat_messages WHERE updatedAt > :after ORDER BY updatedAt ASC")
    abstract suspend fun getMessagesUpdatedAfter(after: Long): List<ChatMessageLocal>

    @Query("SELECT * FROM chat_messages WHERE id = :messageId")
    abstract suspend fun getMessage(messageId: String): ChatMessageLocal?

    @Query("SELECT * FROM chat_messages WHERE chatId = :chatId AND status = :status AND origintype == 'USER'")
    abstract fun subscribeOutgoingMessagesByStatus(
        chatId: ByteArray,
        status: ChatMessageLocal.Status
    ): Flow<List<ChatMessageLocal>>

    @Query("""
        SELECT * FROM chat_messages
        WHERE chatId = :chatId AND type = :type
        AND status = :status
        AND ((:isOutgoing = 1 AND origintype = 'USER') OR (:isOutgoing = 0 AND origintype != 'USER'))
    """)
    abstract fun subscribeMessages(chatId: ByteArray, type: ChatMessageLocal.Type, status: ChatMessageLocal.Status, isOutgoing: Boolean): Flow<List<ChatMessageLocal>>

    @Query("SELECT * FROM chat_messages WHERE id = :messageId")
    abstract fun subscribeMessageById(messageId: String): Flow<ChatMessageLocal?>

    @Query("SELECT * FROM chat_messages WHERE type IN (:types)")
    abstract fun subscribeMessagesByTypes(types: List<ChatMessageLocal.Type>): Flow<List<ChatMessageLocal>>

    @Query("""
        SELECT id, chatId, timestamp, searchableContent FROM chat_messages
        WHERE isInternal = 0
            AND chatId IN (:chatIds)
            AND searchableContent LIKE '%' || :query || '%' ESCAPE '\'
        ORDER BY timestamp DESC
        LIMIT :limit
    """)
    abstract suspend fun searchMessages(
        query: String,
        chatIds: List<ByteArray>,
        limit: Int
    ): List<ChatMessageSearchRow>

    @Query("UPDATE chat_messages SET status = :newStatus, updatedAt = :updatedAt WHERE id = :messageId")
    abstract suspend fun updateStatus(messageId: String, newStatus: ChatMessageLocal.Status, updatedAt: Long): Int

    @Update(entity = ChatMessageLocal::class)
    abstract suspend fun updateMessagesContents(updates: List<MessageContentUpdateLocal>)

    @Query("UPDATE chat_messages SET status = :newStatus, updatedAt = :updatedAt WHERE id IN (:messageIds)")
    abstract suspend fun updateMessagesStatus(messageIds: List<String>, newStatus: ChatMessageLocal.Status, updatedAt: Long): Int

    @Query(
        """
        UPDATE chat_messages SET status = :toStatus
        WHERE chatId = :chatId AND status = :fromStatus AND origintype == 'USER'
        AND NOT EXISTS (SELECT 1 FROM chat_message_compaction_links WHERE originalId = chat_messages.id)
        """
    )
    abstract suspend fun updateOutgoingMessagesStatusForChat(chatId: ByteArray, fromStatus: ChatMessageLocal.Status, toStatus: ChatMessageLocal.Status)

    @Query(
        """
        UPDATE chat_messages SET status = :toStatus
        WHERE chatId = :chatId AND status = :fromStatus AND origintype == 'USER' AND type IN (:types)
        AND NOT EXISTS (SELECT 1 FROM chat_message_compaction_links WHERE originalId = chat_messages.id)
        """
    )
    abstract suspend fun updateOutgoingMessagesStatusForChatWithTypes(chatId: ByteArray, fromStatus: ChatMessageLocal.Status, toStatus: ChatMessageLocal.Status, types: List<ChatMessageLocal.Type>)

    @Query(
        """
        UPDATE chat_messages SET status = :toStatus
        WHERE chatId = :chatId AND status = :fromStatus AND origintype == 'USER' AND type NOT IN (:types)
        AND NOT EXISTS (SELECT 1 FROM chat_message_compaction_links WHERE originalId = chat_messages.id)
        """
    )
    abstract suspend fun updateOutgoingMessagesStatusForChatExcludingTypes(chatId: ByteArray, fromStatus: ChatMessageLocal.Status, toStatus: ChatMessageLocal.Status, types: List<ChatMessageLocal.Type>)

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    protected abstract suspend fun insertPendingExpansions(pending: List<ChatMessagePendingExpansionLocal>)

    @Query("DELETE FROM chat_messages WHERE id IN (:messageIds)")
    protected abstract suspend fun removeMessageRows(messageIds: List<String>)

    @Query("DELETE FROM chat_message_compaction_links WHERE originalId IN (:originalIds)")
    protected abstract suspend fun deleteCompactionLinksOf(originalIds: List<String>)

    @Query("DELETE FROM chat_message_pending_expansions WHERE commitId = :commitId")
    protected abstract suspend fun clearPendingExpansion(commitId: String)

    @Transaction
    open suspend fun remove(messageId: String) {
        removeMessageRows(listOf(messageId))
        deleteCompactionLinksOf(listOf(messageId))
        clearPendingExpansion(messageId)
    }

    @Query("UPDATE chat_messages SET status = 'IS_READ' WHERE chatId = :chatId AND timestamp <= :timestamp AND status = 'NEW' AND origintype != 'USER'")
    abstract suspend fun markMessagesAsReadUpToTimestamp(chatId: ByteArray, timestamp: Long)

    @Query("UPDATE chat_messages SET status = 'IS_READ' WHERE chatId = :chatId AND status = 'NEW' AND origintype != 'USER' AND type IN (:types)")
    abstract suspend fun markMessagesByTypesAsRead(chatId: ByteArray, types: List<String>)

    @Query("UPDATE chat_messages SET status = 'IS_READ' WHERE id = :messageId")
    abstract suspend fun markMessageAsRead(messageId: String)

    @Query("SELECT * FROM chat_messages WHERE chatId = :chatId AND type = 'UNSUPPORTED'")
    abstract suspend fun getUnsupportedMessages(chatId: ByteArray): List<ChatMessageLocal>

    @Query("DELETE FROM chat_messages WHERE chatId = :chatId")
    abstract suspend fun deleteAllMessageRows(chatId: ByteArray)

    @Query("DELETE FROM chat_message_compaction_links WHERE originalId IN (SELECT id FROM chat_messages WHERE chatId = :chatId)")
    abstract suspend fun deleteCompactionLinksForChat(chatId: ByteArray)

    @Query("DELETE FROM chat_message_pending_expansions WHERE commitId IN (SELECT id FROM chat_messages WHERE chatId = :chatId)")
    abstract suspend fun deletePendingExpansionsForChat(chatId: ByteArray)

    @Transaction
    open suspend fun deleteAllMessages(chatId: ByteArray) {
        deleteCompactionLinksForChat(chatId)
        deletePendingExpansionsForChat(chatId)
        deleteAllMessageRows(chatId)
    }

    @Query("SELECT MAX(timestamp) FROM chat_messages WHERE chatId = :chatId")
    abstract suspend fun getLatestTimestamp(chatId: ByteArray): Long?

    @Query("UPDATE chat_messages SET timestamp = timestamp + :delta WHERE chatId = :chatId")
    abstract suspend fun shiftTimestamps(chatId: ByteArray, delta: Long)

    @Transaction
    open suspend fun updateTimestamps(chatId: ByteArray, targetTimestamp: Long) {
        val latest = getLatestTimestamp(chatId) ?: return
        val delta = targetTimestamp - latest
        if (delta != 0L) {
            shiftTimestamps(chatId, delta)
        }
    }

    @Query("SELECT id, status FROM chat_messages WHERE id IN (:messageIds)")
    abstract suspend fun getMessageStatuses(messageIds: List<String>): List<MessageStatusProjection>

    sealed interface Placement {
        data object Latest : Placement

        data object ByTimestamp : Placement

        data class SameAs(val messageId: String) : Placement
    }

    data class MessageStatusProjection(
        val id: String,
        val status: ChatMessageLocal.Status
    )

    data class SortOrderProjection(
        val id: String,
        val sortOrder: Long
    )

    class MessageContentUpdateLocal(
        val id: String,
        val content: ByteArray?,
        val type: ChatMessageLocal.Type,
        val updatedAt: Long
    )

    companion object {
        // SQLITE_MAX_VARIABLE_NUMBER of the SQLite shipped with API 29 and 30
        private const val SQLITE_VARIABLE_LIMIT = 999
    }
}
