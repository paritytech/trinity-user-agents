package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ProductTopUpLocal
import java.math.BigInteger

@Dao
interface ProductTopUpDao {
    /** Fails when the id is already taken, which is what makes registering a top-up idempotent. */
    @Insert(onConflict = OnConflictStrategy.ABORT)
    suspend fun insert(topUp: ProductTopUpLocal)

    @Query("SELECT * FROM product_top_ups WHERE productId = :productId AND topUpId = :topUpId")
    suspend fun get(productId: String, topUpId: String): ProductTopUpLocal?

    /** The top-ups still worth running: the ones that have not reached a verdict. */
    @Query("SELECT * FROM product_top_ups WHERE outcome IS NULL")
    suspend fun getUnfinished(): List<ProductTopUpLocal>

    @Query(
        """
        UPDATE product_top_ups
        SET outcome = :outcome, actualClaimedPlanks = :actualClaimedPlanks
        WHERE productId = :productId AND topUpId = :topUpId
        """
    )
    suspend fun settle(
        productId: String,
        topUpId: String,
        outcome: ProductTopUpLocal.Outcome,
        actualClaimedPlanks: BigInteger?,
    )
}
