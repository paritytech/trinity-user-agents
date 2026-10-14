package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ProductFundingOperationLocal

@Dao
interface ProductFundingOperationDao {
    /** Returns the auto-generated operation id. */
    @Insert
    suspend fun insert(operation: ProductFundingOperationLocal): Long

    @Query("DELETE FROM product_funding_operations WHERE operationId = :operationId")
    suspend fun delete(operationId: Long)

    @Query("SELECT * FROM product_funding_operations")
    suspend fun getAll(): List<ProductFundingOperationLocal>
}
