package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.StatementStoreSlotAllocationLocal

@Dao
interface StatementStoreSlotAllocationDao {
    @Insert(onConflict = OnConflictStrategy.ABORT)
    suspend fun insert(row: StatementStoreSlotAllocationLocal): Long

    @Query("DELETE FROM statement_store_slot_allocations WHERE id = :id")
    suspend fun deleteById(id: Long)

    /**
     * Deletes the row that exactly matches the on-chain `(account, seq)` coordinate of a
     * slot being evicted. No-op when no row matches (the evicted slot was external).
     */
    @Query(
        """
        DELETE FROM statement_store_slot_allocations
        WHERE chainId = :chainId AND collection = :collection
          AND accountId = :accountId AND seq = :seq
        """
    )
    suspend fun deleteSlot(chainId: String, collection: String, accountId: ByteArray, seq: Int)

    @Query("DELETE FROM statement_store_slot_allocations WHERE chainId = :chainId AND accountId = :accountId")
    suspend fun deleteAllForAccount(chainId: String, accountId: ByteArray)

    @Query(
        """
        SELECT * FROM statement_store_slot_allocations
        WHERE chainId = :chainId AND latestRenewedPeriod < :currentPeriod
        """
    )
    suspend fun staleRows(
        chainId: String,
        currentPeriod: Long,
    ): List<StatementStoreSlotAllocationLocal>

    @Query(
        """
        SELECT EXISTS(
            SELECT 1 FROM statement_store_slot_allocations
            WHERE chainId = :chainId AND accountId = :accountId
              AND latestRenewedPeriod < :currentPeriod
        )
        """
    )
    suspend fun hasStaleFor(
        chainId: String,
        accountId: ByteArray,
        currentPeriod: Long,
    ): Boolean

    /**
     * Bulk lookup of "highest priority level seen locally" per account. Returns one row
     * per account that has at least one local row in [accountIds]; accounts with no rows
     * are absent from the result.
     */
    @Query(
        """
        SELECT accountId AS accountId, MAX(priorityLevel) AS maxPriorityLevel
        FROM statement_store_slot_allocations
        WHERE chainId = :chainId AND accountId IN (:accountIds)
        GROUP BY accountId
        """
    )
    suspend fun maxPriorityLevelsFor(
        chainId: String,
        accountIds: Collection<ByteArray>,
    ): List<AccountMaxPriorityRow>

    /**
     * Updates a renewed row to its new on-chain coordinate. `newSeq` is the seq the renewer
     * just assigned in the new period and `newCollection` the collection it claimed it in
     * (renewal may migrate a row across collections); together with `latestRenewedPeriod`
     * they locate the row's current on-chain claim.
     */
    @Query(
        """
        UPDATE statement_store_slot_allocations
        SET latestRenewedPeriod = :period, sinceMillis = :sinceMillis, seq = :newSeq,
            collection = :newCollection
        WHERE id = :id
        """
    )
    suspend fun markRenewed(id: Long, period: Long, sinceMillis: Long, newSeq: Int, newCollection: String)

    class AccountMaxPriorityRow(
        val accountId: ByteArray,
        val maxPriorityLevel: Int,
    )
}
