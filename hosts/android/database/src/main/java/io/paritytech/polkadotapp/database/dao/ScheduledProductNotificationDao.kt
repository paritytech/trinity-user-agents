package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.ScheduledProductNotificationLocal
import uniffi.truapi.ProductId

@Dao
interface ScheduledProductNotificationDao {
    @Insert(onConflict = OnConflictStrategy.ABORT)
    suspend fun insert(notification: ScheduledProductNotificationLocal)

    @Query("SELECT * FROM scheduled_product_notifications")
    suspend fun getAll(): List<ScheduledProductNotificationLocal>

    @Query("SELECT * FROM scheduled_product_notifications WHERE productId = :productId AND notificationId = :notificationId")
    suspend fun find(productId: ProductId, notificationId: Int): ScheduledProductNotificationLocal?

    @Query("SELECT COUNT(*) FROM scheduled_product_notifications")
    suspend fun countAll(): Int

    @Query("SELECT EXISTS(SELECT 1 FROM scheduled_product_notifications WHERE productId = :productId AND notificationId = :notificationId)")
    suspend fun exists(productId: ProductId, notificationId: Int): Boolean

    @Query("DELETE FROM scheduled_product_notifications WHERE productId = :productId AND notificationId = :notificationId")
    suspend fun delete(productId: ProductId, notificationId: Int)

    @Query("DELETE FROM scheduled_product_notifications WHERE productId = :productId")
    suspend fun deleteAllByProduct(productId: ProductId)
}
