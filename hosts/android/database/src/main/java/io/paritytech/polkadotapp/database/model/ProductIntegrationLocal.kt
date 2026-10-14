package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey

@Entity(
    tableName = "product_integrations",
    primaryKeys = ["productId", "type"],
    foreignKeys = [
        ForeignKey(
            entity = ProductLocal::class,
            parentColumns = ["id"],
            childColumns = ["productId"],
            onDelete = ForeignKey.CASCADE
        )
    ]
)
class ProductIntegrationLocal(
    val productId: String,
    val type: String,
    val metadata: String?,
)
