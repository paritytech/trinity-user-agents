package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.DurableTxLocal
import kotlinx.coroutines.flow.Flow

/** Holding whatever their domain locked: submitted and undecided, or waiting to be built. */
private const val LIVE_STATUSES = "('PENDING', 'PENDING_SUCCESS', 'PENDING_SUBMISSION')"

/** What a recovery pass can decide: something was submitted and nothing has concluded about it. */
private const val EVALUABLE_STATUSES = "('PENDING', 'PENDING_SUCCESS')"

@Dao
abstract class DurableTxDao {
    /**
     * Runs [action] in one database transaction.
     *
     * A domain writes its own rows from inside here, so its locks and the transaction row commit together
     * or not at all — there is never an extrinsic in flight without the record holding what it consumes.
     */
    @Transaction
    open suspend fun withTransaction(action: suspend () -> Unit) {
        return action()
    }

    /** Returns the id SQLite assigned, which is also this transaction's place in registration order. */
    @Insert(onConflict = OnConflictStrategy.ABORT)
    abstract suspend fun insert(transaction: DurableTxLocal): Long

    // ---- status ----

    @Query("SELECT status FROM durable_tx WHERE id = :id")
    abstract suspend fun getStatus(id: Long): DurableTxLocal.Status?

    @Query("SELECT status FROM durable_tx WHERE id = :id")
    abstract fun subscribeStatus(id: Long): Flow<DurableTxLocal.Status?>

    /**
     * Returns the number of rows written: 0 means the transaction no longer reads [expected], or is no longer
     * the attempt whose hash is [expectedTxHash] — a verdict about earlier bytes must not land on a rebuilt one.
     */
    @Query(
        """
        UPDATE durable_tx
        SET status = :status,
            successDetectedblockNumber = :successDetectedBlockNumber,
            successDetectedblockHash = :successDetectedBlockHash
        WHERE id = :id AND status = :expected AND txHash = :expectedTxHash
        """
    )
    abstract suspend fun compareAndSetStatus(
        id: Long,
        expected: DurableTxLocal.Status,
        expectedTxHash: String,
        status: DurableTxLocal.Status,
        successDetectedBlockNumber: Long?,
        successDetectedBlockHash: String?,
    ): Int

    /** Starts a new attempt on a row waiting for one. Returns 0 when the row was no longer waiting. */
    @Query(
        """
        UPDATE durable_tx
        SET status = 'PENDING',
            txHash = :txHash,
            checkpointblockNumber = :checkpointBlockNumber,
            checkpointblockHash = :checkpointBlockHash,
            mortalityBlocks = :mortalityBlocks,
            successDetectedblockNumber = NULL,
            successDetectedblockHash = NULL
        WHERE id = :id AND status = 'PENDING_SUBMISSION'
        """
    )
    abstract suspend fun startAttempt(
        id: Long,
        txHash: String,
        checkpointBlockNumber: Long,
        checkpointBlockHash: String,
        mortalityBlocks: Long,
    ): Int

    /** Ends a row waiting to be built for good. Returns 0 when the row was no longer waiting. */
    @Query("UPDATE durable_tx SET status = 'FAILURE' WHERE id = :id AND status = 'PENDING_SUBMISSION'")
    abstract suspend fun abandonSubmission(id: Long): Int

    // ---- reads ----

    @Query("SELECT * FROM durable_tx WHERE id = :id")
    abstract suspend fun get(id: Long): DurableTxLocal?

    @Query("SELECT * FROM durable_tx WHERE domainId = :domainId ORDER BY id ASC")
    abstract suspend fun getAll(domainId: String): List<DurableTxLocal>

    /** Every row that carries an attempt: one waiting to be built has nothing a pass could read about it. */
    @Query("SELECT * FROM durable_tx WHERE domainId = :domainId AND status != 'PENDING_SUBMISSION' ORDER BY id ASC")
    abstract suspend fun getAllSubmitted(domainId: String): List<DurableTxLocal>

    @Query("SELECT * FROM durable_tx WHERE status = 'PENDING_SUBMISSION' ORDER BY id ASC")
    abstract fun subscribePendingSubmissions(): Flow<List<DurableTxLocal>>

    @Query(
        """
        SELECT * FROM durable_tx
        WHERE status = 'PENDING_SUBMISSION' AND submissionPolicyId = :policyId
          AND operationGroupId IS :groupId
        ORDER BY id ASC
        """
    )
    abstract suspend fun getPendingSubmissions(policyId: String, groupId: String?): List<DurableTxLocal>

    @Query("SELECT EXISTS(SELECT 1 FROM durable_tx WHERE status IN $LIVE_STATUSES)")
    abstract suspend fun hasLiveTransactions(): Boolean

    /** Only these domains need their oracle opened; the rest have nothing a pass could decide. */
    @Query("SELECT DISTINCT domainId FROM durable_tx WHERE status IN $EVALUABLE_STATUSES")
    abstract suspend fun liveDomains(): List<String>

    @Query(
        """
        SELECT * FROM durable_tx
        WHERE domainId = :domainId AND operationGroupId = :groupId
        ORDER BY id ASC
        """
    )
    abstract suspend fun getGroup(domainId: String, groupId: String): List<DurableTxLocal>

    @Query(
        """
        SELECT * FROM durable_tx
        WHERE domainId = :domainId AND operationGroupId = :groupId
        ORDER BY id ASC
        """
    )
    abstract fun subscribeGroup(domainId: String, groupId: String): Flow<List<DurableTxLocal>>
}
