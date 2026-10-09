package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey
import java.math.BigInteger

/** One ended funding session, kept once the TrUAPI core has handed it over. */
@Entity(tableName = "product_funding_records")
class ProductFundingRecordLocal(
    @PrimaryKey val intent: String,
    val direction: Direction,
    val rail: Rail?,
    val asset: String?,
    val providerId: String?,
    val requestedPlanks: BigInteger?,
    val settledPlanks: BigInteger?,
    val outcome: Outcome,
    val failureCode: String?,
    val payout: Payout?,
    val payoutFailureReason: String?,
    val transactionId: String?,
    val reference: String?,
    val openedAtMillis: Long,
    val settledAtMillis: Long,
) {
    enum class Direction {
        IN,
        OUT,
    }

    enum class Rail {
        CARD,
        BANK,
        CRYPTO,
    }

    enum class Outcome {
        DELIVERED,
        RELEASED,
        REFUNDED,
        FAILED,
    }

    enum class Payout {
        PAID_OUT,
        FAILED,
    }
}
