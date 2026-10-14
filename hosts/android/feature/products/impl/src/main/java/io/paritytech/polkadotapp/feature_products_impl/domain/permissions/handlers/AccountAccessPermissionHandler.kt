package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRequester
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import javax.inject.Inject

class AccountAccessPermissionHandler @Inject constructor(
    private val repository: ProductPermissionRepository,
    private val requester: ProductPermissionRequester,
) : ProductPermissionHandler<ProductPermission.AccountAccess> {
    override suspend fun isGranted(productId: ProductId, permission: ProductPermission.AccountAccess): Boolean {
        if (permission.targetProductId == productId.value) return true
        return repository.isGranted(productId, permission)
    }

    override suspend fun request(productId: ProductId, permission: ProductPermission.AccountAccess): Boolean {
        if (isGranted(productId, permission)) return true

        return when (requester.prompt(productId, permission)) {
            PermissionDecision.AllowAlways -> {
                repository.grant(productId, permission)
                true
            }
            PermissionDecision.AllowOnce -> {
                repository.grantOneTime(productId, permission)
                true
            }
            PermissionDecision.Deny -> false
        }
    }

    override suspend fun revoke(productId: ProductId, permission: ProductPermission.AccountAccess) {
        repository.revoke(productId, permission)
    }
}
