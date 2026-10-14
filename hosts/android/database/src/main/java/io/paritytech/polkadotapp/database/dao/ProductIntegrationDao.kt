package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ProductIntegrationLocal
import io.paritytech.polkadotapp.database.model.ProductLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface ProductIntegrationDao {
    @Query("SELECT * FROM product_integrations WHERE productId = :productId")
    fun observeByProduct(productId: String): Flow<List<ProductIntegrationLocal>>

    @Query(
        """
        SELECT p.* FROM products p
        INNER JOIN product_integrations pi ON p.id = pi.productId
        WHERE pi.type = :type
        """
    )
    fun observeProductsByIntegrationType(type: String): Flow<List<ProductLocal>>

    @Query("SELECT * FROM product_integrations WHERE productId = :productId AND type = :type")
    suspend fun get(productId: String, type: String): ProductIntegrationLocal?

    @Query("SELECT * FROM product_integrations WHERE type = :type")
    fun observeByType(type: String): Flow<List<ProductIntegrationLocal>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(integration: ProductIntegrationLocal)

    @Query("DELETE FROM product_integrations WHERE productId = :productId AND type = :type")
    suspend fun delete(productId: String, type: String)

    @Query("DELETE FROM product_integrations WHERE productId = :productId")
    suspend fun deleteAllByProduct(productId: String)
}
