package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission

class AutoAllowProductPermissionRequester(
    private val whitelistedProductsProvider: WhitelistedProductsProvider,
    private val delegate: ProductPermissionRequester,
) : ProductPermissionRequester {
    override suspend fun prompt(productId: ProductId, permission: ProductPermission): PermissionDecision {
        if (productId.isAutoAllowed()) return PermissionDecision.AllowAlways

        return delegate.prompt(productId, permission)
    }

    override suspend fun promptBatched(
        productId: ProductId,
        permissions: List<RemotePermission>,
    ): PermissionDecision {
        if (productId.isAutoAllowed()) return PermissionDecision.AllowAlways

        return delegate.promptBatched(productId, permissions)
    }

    private suspend fun ProductId.isAutoAllowed(): Boolean {
        return this in whitelistedProductsProvider.whitelistedProducts()
    }
}
