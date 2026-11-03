package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers

import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRequester
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import uniffi.truapi.ProductId
import javax.inject.Inject

class NftPurseReadPermissionHandler @Inject constructor(
    private val repository: ProductPermissionRepository,
    private val requester: ProductPermissionRequester,
) : ProductPermissionHandler<ProductPermission.NftPurseRead> {
    override suspend fun isGranted(productId: ProductId, permission: ProductPermission.NftPurseRead): Boolean {
        return repository.isGranted(productId, permission)
    }

    override suspend fun request(productId: ProductId, permission: ProductPermission.NftPurseRead): Boolean {
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

    override suspend fun revoke(productId: ProductId, permission: ProductPermission.NftPurseRead) {
        repository.revoke(productId, permission)
    }
}
