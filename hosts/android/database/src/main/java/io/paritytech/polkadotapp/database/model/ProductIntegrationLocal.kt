package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey
import uniffi.truapi.ProductId

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
    val productId: ProductId,
    val type: String,
    val metadata: String?,
)
