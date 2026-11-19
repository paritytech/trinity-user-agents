package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.FileUploadLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface FileUploadDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(upload: FileUploadLocal)

    @Query("SELECT * FROM file_uploads WHERE messageId = :messageId")
    suspend fun get(messageId: String): FileUploadLocal?

    @Query(
        """SELECT messageId FROM file_uploads
        WHERE status = 'PENDING'
            AND (nextAttemptAt IS NULL OR nextAttemptAt <= :now)
        ORDER BY createdAt ASC LIMIT 1"""
    )
    suspend fun getNextDueId(now: Long): String?

    @Transaction
    suspend fun claimNextDue(now: Long): FileUploadLocal? {
        val messageId = getNextDueId(now) ?: return null
        markInProgress(messageId)
        return get(messageId)
    }

    @Query("UPDATE file_uploads SET status = 'PENDING' WHERE status = 'IN_PROGRESS'")
    suspend fun releaseInProgress()

    @Query("SELECT MIN(nextAttemptAt) FROM file_uploads WHERE status = 'PENDING' AND nextAttemptAt IS NOT NULL")
    suspend fun getNextBackoffEndAt(): Long?

    @Query("SELECT messageId FROM file_uploads WHERE status = 'PENDING'")
    fun observePendingIds(): Flow<List<String>>

    @Query("SELECT * FROM file_uploads WHERE status IN ('PENDING', 'IN_PROGRESS', 'FAILED')")
    fun observeActive(): Flow<List<FileUploadLocal>>

    @Query("UPDATE file_uploads SET status = 'IN_PROGRESS' WHERE messageId = :messageId")
    suspend fun markInProgress(messageId: String)

    @Query("UPDATE file_uploads SET fileSize = :fileSize, totalChunks = :totalChunks WHERE messageId = :messageId")
    suspend fun updateFileInfo(messageId: String, fileSize: Long, totalChunks: Int)

    @Query("UPDATE file_uploads SET uploadedChunks = :uploadedChunks, chunkHashes = :chunkHashes WHERE messageId = :messageId")
    suspend fun updateProgress(messageId: String, uploadedChunks: Int, chunkHashes: String)

    @Query(
        """UPDATE file_uploads
        SET status = 'PENDING',
            attemptCount = :attemptCount,
            firstFailureAt = :firstFailureAt,
            nextAttemptAt = :nextAttemptAt,
            errorCategory = NULL,
            errorCause = NULL,
            uploadedChunks = CASE WHEN nodeUrl = :nodeUrl THEN uploadedChunks ELSE 0 END,
            chunkHashes = CASE WHEN nodeUrl = :nodeUrl THEN chunkHashes ELSE NULL END,
            nodeUrl = :nodeUrl
        WHERE messageId = :messageId"""
    )
    suspend fun scheduleRetry(
        messageId: String,
        nodeUrl: String,
        attemptCount: Int,
        firstFailureAt: Long,
        nextAttemptAt: Long
    )

    @Query("SELECT EXISTS(SELECT 1 FROM file_uploads WHERE messageId = :messageId AND status = 'IN_PROGRESS')")
    suspend fun isInProgress(messageId: String): Boolean

    @Query("UPDATE file_uploads SET status = 'DONE' WHERE messageId = :messageId")
    suspend fun markDone(messageId: String)

    @Query("UPDATE file_uploads SET status = 'FAILED', errorCategory = :errorCategory, errorCause = :errorCause WHERE messageId = :messageId")
    suspend fun markFailed(messageId: String, errorCategory: FileUploadLocal.ErrorCategory, errorCause: String)
}
