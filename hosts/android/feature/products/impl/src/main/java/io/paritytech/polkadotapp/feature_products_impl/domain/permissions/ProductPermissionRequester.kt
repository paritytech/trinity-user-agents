package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission
import uniffi.truapi.ProductId

interface ProductPermissionRequester {
    suspend fun prompt(productId: ProductId, permission: ProductPermission): PermissionDecision

    suspend fun promptBatched(productId: ProductId, permissions: List<RemotePermission>): PermissionDecision
}
