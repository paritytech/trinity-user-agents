package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import java.math.BigInteger

@Entity(tableName = "external_payments", primaryKeys = ["origin", "id"])
class ExternalPaymentLocal(
    val id: String,
    val origin: String,
    val amountPlanks: BigInteger,
    val destination: ByteArray,
    val stage: Stage,
    val failureReason: String?,
    /** SCALE-encoded list of voucher ring-vrf-key indices: selected for offboard, or kept aside while recycling. */
    val selectedVoucherKeys: String?,
    val surplusPlanks: BigInteger?,
    val claimedPlanks: BigInteger?,
    val createdAt: Long,
    val updatedAt: Long,
) {
    enum class Stage {
        ENSURE_VOUCHERS,
        AWAIT_RECYCLING,
        OFFBOARD_VOUCHERS,
        COMPLETED,
        PARTIALLY_COMPLETED,
        FAILED,
    }
}
