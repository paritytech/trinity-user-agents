package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy.Companion.REPLACE
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ProductPermissionGrantLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface ProductPermissionGrantDao {
    @Query("SELECT * FROM product_permission_grants WHERE productId = :productId AND permissionType = :permissionType AND permissionKey = :permissionKey")
    suspend fun get(productId: String, permissionType: String, permissionKey: String): ProductPermissionGrantLocal?

    @Query("SELECT * FROM product_permission_grants WHERE productId = :productId AND permissionType = :permissionType")
    suspend fun getAllByType(productId: String, permissionType: String): List<ProductPermissionGrantLocal>

    @Query("SELECT * FROM product_permission_grants WHERE productId = :productId")
    suspend fun getAllByProduct(productId: String): List<ProductPermissionGrantLocal>

    @Query("SELECT * FROM product_permission_grants WHERE productId = :productId")
    fun observeAllByProduct(productId: String): Flow<List<ProductPermissionGrantLocal>>

    @Query("SELECT EXISTS(SELECT 1 FROM product_permission_grants WHERE productId = :productId)")
    fun observeHasAnyPermissionRequested(productId: String): Flow<Boolean>

    @Insert(onConflict = REPLACE)
    suspend fun insert(grant: ProductPermissionGrantLocal)

    @Query("UPDATE product_permission_grants SET granted = 0, grantedAt = NULL WHERE productId = :productId AND permissionType = :permissionType AND permissionKey = :permissionKey")
    suspend fun revoke(productId: String, permissionType: String, permissionKey: String)

    @Query("DELETE FROM product_permission_grants WHERE productId = :productId AND permissionType = :permissionType AND permissionKey = :permissionKey")
    suspend fun delete(productId: String, permissionType: String, permissionKey: String)

    @Query("SELECT EXISTS(SELECT 1 FROM product_permission_grants WHERE productId = :productId AND permissionType = :permissionType AND permissionKey IN (:permissionKeys) AND granted = 1)")
    suspend fun isAnyGranted(productId: String, permissionType: String, permissionKeys: List<String>): Boolean

    @Query("DELETE FROM product_permission_grants WHERE productId = :productId")
    suspend fun deleteAllByProduct(productId: String)
}
