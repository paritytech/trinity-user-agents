package io.paritytech.polkadotapp.database.model

import androidx.room.Entity

@Entity(
    tableName = "product_permission_grants",
    primaryKeys = ["productId", "permissionType", "permissionKey"]
)
class ProductPermissionGrantLocal(
    val productId: String,
    val permissionType: String,
    val permissionKey: String,
    val granted: Boolean,
    val grantedAt: Long?,
)
