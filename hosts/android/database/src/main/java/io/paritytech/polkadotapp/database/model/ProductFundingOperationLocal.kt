package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

/**
 * An open product funding operation. Persisted so a funding flow can resume after an app restart:
 * on start the worker for each open operation is re-acquired. Deleted when the operation ends.
 */
@Entity(tableName = "product_funding_operations")
class ProductFundingOperationLocal(
    @PrimaryKey(autoGenerate = true) val operationId: Long,
    val productId: String,
    val label: String?,
)
