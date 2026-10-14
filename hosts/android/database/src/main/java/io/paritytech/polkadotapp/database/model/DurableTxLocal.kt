package io.paritytech.polkadotapp.database.model

import androidx.room.Embedded
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import io.paritytech.polkadotapp.database.model.DurableTxLocal.Companion.TABLE_NAME

/**
 * One transaction the durability engine has taken responsibility for.
 *
 * Domain-neutral by construction: the bytes' hash, the window they are valid in, and what the ledger has
 * concluded. Whatever a domain locks for a transaction lives in that domain's own tables, keyed on [id] and
 * written inside the same transaction as this row.
 *
 * [txHash], [checkpoint] and [mortalityBlocks] describe the current attempt, and are null for a row that was
 * scheduled and has not been built yet. A retry overwrites them in place, so the domain's rows stay attached
 * to the same id across attempts.
 *
 * Signed bytes are deliberately not persisted here — nothing in this table can be replayed onto the chain.
 * Rows are never deleted.
 */
@Entity(
    tableName = TABLE_NAME,
    indices = [
        Index(value = ["domainId", "operationGroupId"]),
        Index(value = ["domainId", "status"]),
    ],
)
class DurableTxLocal(
    /** Registration order, and the key a domain's own rows reference. */
    @PrimaryKey(autoGenerate = true)
    val id: Long,
    val domainId: String,
    val operationGroupId: String?,
    val txHash: String?,
    @Embedded(prefix = "checkpoint") val checkpoint: BlockRefLocal?,
    val mortalityBlocks: Long?,
    @Embedded(prefix = "successDetected") val successDetectedAt: BlockRefLocal?,
    val status: Status,
    /** Which submission policy builds this transaction again; null for one that is never retried. */
    val submissionPolicyId: String?,
    /** Opaque to the engine: whatever [submissionPolicyId]'s policy needs to build it. */
    val submissionPolicyParams: ByteArray?,
) {
    enum class Status { PENDING, PENDING_SUCCESS, FINALIZED_SUCCESS, FAILURE, PENDING_SUBMISSION }

    companion object {
        const val TABLE_NAME = "durable_tx"

        const val UNSAVED_ID = 0L
    }
}
