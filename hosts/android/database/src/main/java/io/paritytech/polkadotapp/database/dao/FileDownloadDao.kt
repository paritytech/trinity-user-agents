package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.FileDownloadLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface FileDownloadDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(download: FileDownloadLocal)

    @Query(
        """SELECT * FROM file_downloads
        WHERE status IN ('PENDING', 'IN_PROGRESS')
            AND (nextAttemptAt IS NULL OR nextAttemptAt <= :now)
        ORDER BY createdAt ASC LIMIT 1"""
    )
    suspend fun getNextPending(now: Long): FileDownloadLocal?

    @Query(
        """SELECT MIN(nextAttemptAt) FROM file_downloads
        WHERE status IN ('PENDING', 'IN_PROGRESS') AND nextAttemptAt IS NOT NULL"""
    )
    suspend fun getSoonestNextAttemptAt(): Long?

    @Query("SELECT * FROM file_downloads WHERE status IN ('PENDING', 'IN_PROGRESS', 'FAILED', 'CANCELLED')")
    fun observeActive(): Flow<List<FileDownloadLocal>>

    @Query("UPDATE file_downloads SET status = 'IN_PROGRESS' WHERE messageId = :messageId")
    suspend fun markInProgress(messageId: String)

    @Query(
        """UPDATE file_downloads
        SET payloadKind = 'INLINE',
            filePath = :filePath
        WHERE messageId = :messageId"""
    )
    suspend fun resolveInline(
        messageId: String,
        filePath: String
    )

    @Query(
        """UPDATE file_downloads
        SET payloadKind = 'CHUNKED',
            chunkHashes = :chunkHashes
        WHERE messageId = :messageId"""
    )
    suspend fun resolveChunked(
        messageId: String,
        chunkHashes: String
    )

    @Query(
        """UPDATE file_downloads
        SET downloadedChunks = :downloadedChunks,
            filePath = :filePath
        WHERE messageId = :messageId"""
    )
    suspend fun updateProgress(
        messageId: String,
        downloadedChunks: Int,
        filePath: String
    )

    @Query(
        """UPDATE file_downloads
        SET status = 'PENDING',
            attemptCount = :attemptCount,
            firstFailureAt = :firstFailureAt,
            nextAttemptAt = :nextAttemptAt,
            errorCategory = NULL,
            errorCause = NULL
        WHERE messageId = :messageId"""
    )
    suspend fun scheduleRetry(
        messageId: String,
        attemptCount: Int,
        firstFailureAt: Long,
        nextAttemptAt: Long
    )

    @Query(
        """UPDATE file_downloads
        SET status = 'PENDING',
            attemptCount = 0,
            firstFailureAt = NULL,
            nextAttemptAt = NULL,
            errorCategory = NULL,
            errorCause = NULL
        WHERE messageId = :messageId"""
    )
    suspend fun resetToPending(messageId: String)

    @Query("SELECT EXISTS(SELECT 1 FROM file_downloads WHERE messageId = :messageId AND status = 'IN_PROGRESS')")
    suspend fun isInProgress(messageId: String): Boolean

    @Query("UPDATE file_downloads SET status = 'CANCELLED' WHERE messageId = :messageId")
    suspend fun markCancelled(messageId: String)

    @Query("UPDATE file_downloads SET status = 'DONE' WHERE messageId = :messageId")
    suspend fun markDone(messageId: String)

    @Query("UPDATE file_downloads SET status = 'FAILED', errorCategory = :errorCategory, errorCause = :errorCause WHERE messageId = :messageId")
    suspend fun markFailed(messageId: String, errorCategory: FileDownloadLocal.ErrorCategory, errorCause: String)
}
