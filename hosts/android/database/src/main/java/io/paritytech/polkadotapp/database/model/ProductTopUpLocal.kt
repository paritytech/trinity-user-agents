package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import java.math.BigInteger

@Entity(tableName = "product_top_ups", primaryKeys = ["productId", "topUpId"])
class ProductTopUpLocal(
    val productId: String,
    val topUpId: String,
    val amountPlanks: BigInteger,
    val startedAtMillis: Long,
    val outcome: Outcome?,
    val actualClaimedPlanks: BigInteger?,
) {
    enum class Outcome {
        CLAIMED,
        CLAIMED_PARTIALLY,
        NOT_CLAIMED,
    }
}
