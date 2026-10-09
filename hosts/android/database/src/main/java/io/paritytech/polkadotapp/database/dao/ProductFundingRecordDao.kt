package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Query
import androidx.room.Upsert
import io.paritytech.polkadotapp.database.model.ProductFundingRecordLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface ProductFundingRecordDao {
    /** Rewrites the row when a payout outcome arrives for a session already kept. */
    @Upsert
    suspend fun upsert(record: ProductFundingRecordLocal)

    @Query("SELECT * FROM product_funding_records ORDER BY settledAtMillis DESC")
    fun observeAll(): Flow<List<ProductFundingRecordLocal>>

    @Query("SELECT * FROM product_funding_records ORDER BY settledAtMillis DESC")
    suspend fun getAll(): List<ProductFundingRecordLocal>
}
