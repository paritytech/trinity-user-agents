package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo
import androidx.room.Entity

@Entity(tableName = "recycler_vouchers", primaryKeys = ["installationId", "ringVrfKeyIndex"])
class RecyclerVoucherLocal(
    val installationId: ByteArray,
    val ringVrfKeyIndex: Int,
    val ringVrfPublicKey: ByteArray,
    val recyclerValue: Int,
    val locationRecyclerIndex: Int?,
    val recyclerMembers: Int?,
    val enteredAt: Long?,
    /** Percentage in `0..100`, kept current while the voucher sits in a ring. */
    @ColumnInfo(defaultValue = UNKNOWN_FUNGIBILITY)
    val recyclerFungibility: Int,
    /**
     * Percentage in `0..100`, frozen on the voucher's first transition into a ring, or null before that.
     *
     * Null rather than zero for "not frozen yet", because zero is itself a legitimate frozen value — a ring
     * that was already fully drained when the voucher landed in it. Conflating the two would let a later
     * tick overwrite a real historical maximum, and that maximum is exactly what makes the overloaded
     * `current > max` state meaningful.
     */
    val maxRecyclerFungibility: Int?,
)

/** Must match the `DEFAULT` in `Migration63To64`, or Room's schema validation rejects a migrated database. */
private const val UNKNOWN_FUNGIBILITY = "0"
