package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey

@Entity(
    tableName = "scheduled_product_notifications",
    primaryKeys = ["productId", "notificationId"],
    foreignKeys = [
        ForeignKey(
            entity = ProductLocal::class,
            parentColumns = ["id"],
            childColumns = ["productId"],
            onDelete = ForeignKey.CASCADE
        )
    ]
)
class ScheduledProductNotificationLocal(
    val productId: String,
    val notificationId: Int,
    val text: String,
    val deeplink: String?,
    val scheduledAtMs: Long,
)
