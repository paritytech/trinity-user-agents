package io.paritytech.polkadotapp.feature_products_api.domain.product

import kotlinx.coroutines.flow.Flow

data class PendingProductUpdate(
    val productName: String,
    val iconUri: String,
    val dotNsName: String,
)

interface ProductUpdateNotices {
    fun observeNotice(): Flow<PendingProductUpdate?>

    suspend fun update(pendingUpdate: PendingProductUpdate)

    fun dismiss(pendingUpdate: PendingProductUpdate)
}
