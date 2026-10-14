package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Embedded
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.ChatMessageCompactionLinkLocal
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatMessagePendingExpansionLocal

@Dao
abstract class ChatMessageCompactionDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    protected abstract suspend fun insertCompactionLinks(links: List<ChatMessageCompactionLinkLocal>)

    @Query("SELECT DISTINCT commitId FROM chat_message_compaction_links WHERE originalId IN (:messageIds)")
    abstract suspend fun getLinkedCommitIds(messageIds: List<String>): List<String>

    @Query("SELECT id FROM chat_messages WHERE id IN (:commitIds) AND type = 'COMPACTION_COMMIT' AND status = 'NEW'")
    abstract suspend fun getNeverSentCommitIds(commitIds: List<String>): List<String>

    @Query("DELETE FROM chat_messages WHERE id IN (:messageIds)")
    abstract suspend fun removeMessageRows(messageIds: List<String>)

    @Query("DELETE FROM chat_message_compaction_links WHERE originalId IN (:originalIds)")
    abstract suspend fun deleteCompactionLinksOf(originalIds: List<String>)

    @Transaction
    open suspend fun commitCompaction(messageIds: List<String>, compactionId: String) {
        val previousCommitIds = getLinkedCommitIds(messageIds).filter { it != compactionId }
        insertCompactionLinks(messageIds.map { ChatMessageCompactionLinkLocal(originalId = it, commitId = compactionId) })

        if (previousCommitIds.isNotEmpty()) {
            val orphanedCommitIds = getNeverSentCommitIds(previousCommitIds)
            if (orphanedCommitIds.isNotEmpty()) {
                removeMessageRows(orphanedCommitIds)
                deleteCompactionLinksOf(orphanedCommitIds)
            }
        }
    }

    @Query(
        """
        WITH RECURSIVE compaction_descendants(id) AS (
            SELECT originalId FROM chat_message_compaction_links WHERE commitId = :commitId
            UNION
            SELECT links.originalId FROM chat_message_compaction_links links
            JOIN compaction_descendants ON links.commitId = compaction_descendants.id
        )
        SELECT id FROM compaction_descendants
        """
    )
    abstract suspend fun getCompactionDescendantIds(commitId: String): List<String>

    @Query("UPDATE chat_messages SET status = :toStatus, updatedAt = :updatedAt WHERE id IN (:messageIds) AND status IN (:fromStatuses)")
    abstract suspend fun updateMessagesStatusFrom(messageIds: List<String>, fromStatuses: List<ChatMessageLocal.Status>, toStatus: ChatMessageLocal.Status, updatedAt: Long)

    @Transaction
    open suspend fun propagateStatusToCompactionDescendants(
        commitId: String,
        fromStatuses: List<ChatMessageLocal.Status>,
        toStatus: ChatMessageLocal.Status,
        updatedAt: Long
    ) {
        val descendantIds = getCompactionDescendantIds(commitId)
        if (descendantIds.isNotEmpty()) {
            updateMessagesStatusFrom(descendantIds, fromStatuses, toStatus, updatedAt)
        }
    }

    @Query("SELECT id FROM chat_messages WHERE chatId = :chatId AND type = 'COMPACTION_COMMIT' AND status = 'IS_SENT' AND origintype == 'USER'")
    abstract suspend fun getUnackedCommitIds(chatId: ByteArray): List<String>

    @Query("SELECT id FROM chat_messages WHERE id IN (:messageIds) AND type = 'COMPACTION_COMMIT'")
    abstract suspend fun filterCommitIds(messageIds: List<String>): List<String>

    @Query("DELETE FROM chat_message_compaction_links WHERE commitId IN (:commitIds)")
    abstract suspend fun deleteCompactionLinksByCommitIds(commitIds: List<String>)

    @Transaction
    open suspend fun unwindUnackedCompactions(chatId: ByteArray, updatedAt: Long) {
        for (topCommitId in getUnackedCommitIds(chatId)) {
            val descendantIds = getCompactionDescendantIds(topCommitId)
            val nestedCommitIds = filterCommitIds(descendantIds)
            val leafIds = descendantIds - nestedCommitIds.toSet()

            if (leafIds.isNotEmpty()) {
                updateMessagesStatusFrom(
                    messageIds = leafIds,
                    fromStatuses = listOf(ChatMessageLocal.Status.IS_SENT),
                    toStatus = ChatMessageLocal.Status.NEW,
                    updatedAt = updatedAt
                )
            }

            val commitIds = nestedCommitIds + topCommitId
            removeMessageRows(commitIds)
            deleteCompactionLinksByCommitIds(commitIds)
        }
    }

    @Query(
        """
        SELECT * FROM chat_message_pending_expansions
        JOIN chat_messages ON chat_messages.id = chat_message_pending_expansions.commitId
        WHERE chat_message_pending_expansions.nextAttemptAt IS NULL OR chat_message_pending_expansions.nextAttemptAt <= :now
        ORDER BY chat_messages.timestamp ASC
        LIMIT 1
        """
    )
    abstract suspend fun getNextPendingExpansion(now: Long): PendingExpansionWithCommitLocal?

    @Query(
        """
        UPDATE chat_message_pending_expansions
        SET attemptCount = :attemptCount, firstFailureAt = :firstFailureAt, nextAttemptAt = :nextAttemptAt
        WHERE commitId = :commitId
        """
    )
    abstract suspend fun scheduleExpansionRetry(commitId: String, attemptCount: Int, firstFailureAt: Long, nextAttemptAt: Long)

    @Query("SELECT MIN(nextAttemptAt) FROM chat_message_pending_expansions WHERE nextAttemptAt IS NOT NULL")
    abstract suspend fun getSoonestExpansionNextAttemptAt(): Long?

    @Query("DELETE FROM chat_message_pending_expansions WHERE commitId = :commitId")
    abstract suspend fun clearPendingExpansion(commitId: String)

    @Query("UPDATE chat_messages SET content = :content, type = 'COMPACTION_UNAVAILABLE', isInternal = 0, updatedAt = :updatedAt WHERE id = :messageId")
    abstract suspend fun rewriteAsCompactionUnavailable(messageId: String, content: ByteArray, updatedAt: Long)

    @Transaction
    open suspend fun markCompactionUnrecoverable(messageId: String, content: ByteArray, updatedAt: Long) {
        rewriteAsCompactionUnavailable(messageId, content, updatedAt)
        clearPendingExpansion(messageId)
    }

    class PendingExpansionWithCommitLocal(
        @Embedded val expansion: ChatMessagePendingExpansionLocal,
        @Embedded val commit: ChatMessageLocal
    )
}
