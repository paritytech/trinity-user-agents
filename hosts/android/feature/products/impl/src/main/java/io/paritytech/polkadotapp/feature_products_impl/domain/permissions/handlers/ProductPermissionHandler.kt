package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers

import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import uniffi.truapi.ProductId

interface ProductPermissionHandler<T : ProductPermission> {
    suspend fun isGranted(productId: ProductId, permission: T): Boolean

    suspend fun request(productId: ProductId, permission: T): Boolean

    suspend fun revoke(productId: ProductId, permission: T)
}
