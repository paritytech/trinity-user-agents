package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Query
import androidx.room.Upsert
import io.paritytech.polkadotapp.database.model.TrackedExtrinsicLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface TrackedExtrinsicDao {
    @Upsert
    suspend fun upsert(extrinsic: TrackedExtrinsicLocal)

    @Query("SELECT * FROM tracked_extrinsics WHERE tag = :tag")
    fun observe(tag: String): Flow<TrackedExtrinsicLocal?>

    // Non-terminal rows: the worker re-watches these (incl. ACCEPTED/IN_BLOCK) after a restart.
    @Query("SELECT * FROM tracked_extrinsics WHERE status NOT IN ('FINALIZED', 'FAILED')")
    suspend fun getUnfinished(): List<TrackedExtrinsicLocal>

    @Query(
        "SELECT * FROM tracked_extrinsics WHERE tag LIKE :prefix || '%' AND status NOT IN ('FINALIZED', 'FAILED') " +
            "ORDER BY createdAt DESC LIMIT 1"
    )
    suspend fun getLatestActiveByPrefix(prefix: String): TrackedExtrinsicLocal?

    @Query(
        "SELECT * FROM tracked_extrinsics WHERE tag LIKE :prefix || '%' AND status NOT IN ('FINALIZED', 'FAILED') " +
            "ORDER BY createdAt DESC LIMIT 1"
    )
    fun observeLatestActiveByPrefix(prefix: String): Flow<TrackedExtrinsicLocal?>

    @Query("UPDATE tracked_extrinsics SET status = :status, blockHash = :blockHash, errorMessage = :errorMessage WHERE tag = :tag")
    suspend fun updateStatus(tag: String, status: String, blockHash: String?, errorMessage: String?)
}
