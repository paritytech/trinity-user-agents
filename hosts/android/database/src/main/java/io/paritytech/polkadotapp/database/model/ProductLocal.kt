package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "products")
class ProductLocal(
    @PrimaryKey val id: String,
    // The product id until resolved, then the root-manifest displayName.
    val name: String,
    val iconCid: String?,
    val iconFormat: String?,
    // Debug-menu override: the resolver prefers the chain's worker.<base> record over this.
    val userWorkerUrl: String?,
)
