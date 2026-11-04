package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.ProductIntegrationLocal
import io.paritytech.polkadotapp.database.model.ProductLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class ProductIntegrationDao {
    @Query("SELECT * FROM product_integrations WHERE productId = :productId")
    abstract fun observeByProduct(productId: String): Flow<List<ProductIntegrationLocal>>

    @Query(
        """
        SELECT p.* FROM products p
        INNER JOIN product_integrations pi ON p.id = pi.productId
        WHERE pi.type = :type
        """
    )
    abstract fun observeProductsByIntegrationType(type: String): Flow<List<ProductLocal>>

    @Query("SELECT * FROM product_integrations WHERE productId = :productId AND type = :type")
    abstract suspend fun get(productId: String, type: String): ProductIntegrationLocal?

    @Query("SELECT * FROM product_integrations WHERE type = :type")
    abstract fun observeByType(type: String): Flow<List<ProductIntegrationLocal>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    abstract suspend fun insert(integration: ProductIntegrationLocal)

    @Query("DELETE FROM product_integrations WHERE productId = :productId AND type = :type")
    abstract suspend fun delete(productId: String, type: String)

    @Query("DELETE FROM product_integrations WHERE productId = :productId")
    abstract suspend fun deleteAllByProduct(productId: String)

    @Transaction
    open suspend fun withTransaction(action: suspend () -> Unit) {
        return action()
    }
}
