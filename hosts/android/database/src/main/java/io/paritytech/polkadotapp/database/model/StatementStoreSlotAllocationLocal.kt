package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey

/**
 * One row per on-chain statement-store slot allocation owned by this device. Used to
 * renew slots across period rollovers and to project an account's effective priority
 * during allocate-time eviction filtering.
 */
@Entity(
    tableName = "statement_store_slot_allocations",
    indices = [
        Index("chainId", "collection"),
        Index("chainId", "collection", "accountId"),
    ],
)
class StatementStoreSlotAllocationLocal(
    @PrimaryKey(autoGenerate = true) val id: Long,
    val chainId: String,
    val collection: String,
    val accountId: ByteArray,
    /**
     * Current on-chain seq this row claims, scoped to [latestRenewedPeriod]. Updated by the
     * renewer when the row migrates to a new period; reset on every successful renew/allocate.
     * Combined with [accountId] it uniquely identifies an on-chain slot the device owns —
     * eviction deletes by `(accountId, seq)`, not by an implicit sort.
     */
    val seq: Int,
    val latestRenewedPeriod: Long,
    val sinceMillis: Long,
    val priorityLevel: Int,
)
