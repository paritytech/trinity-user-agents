package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRequester
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission
import javax.inject.Inject

class RemotePermissionHandler @Inject constructor(
    private val networkAccessHandler: NetworkAccessPermissionHandler,
    private val repository: ProductPermissionRepository,
    private val requester: ProductPermissionRequester,
) : ProductPermissionHandler<RemotePermission> {
    override suspend fun isGranted(productId: ProductId, permission: RemotePermission): Boolean {
        return when (permission) {
            is RemotePermission.NetworkAccess -> networkAccessHandler.isGranted(productId, permission)
            is RemotePermission.WebRtcAccess,
            is RemotePermission.ChainSubmitAccess,
            is RemotePermission.StatementSubmitAccess,
            is RemotePermission.PreimageSubmitAccess -> repository.isGranted(productId, permission)
        }
    }

    override suspend fun request(productId: ProductId, permission: RemotePermission): Boolean {
        return when (permission) {
            is RemotePermission.NetworkAccess -> networkAccessHandler.request(productId, permission)
            is RemotePermission.WebRtcAccess,
            is RemotePermission.ChainSubmitAccess,
            is RemotePermission.StatementSubmitAccess,
            is RemotePermission.PreimageSubmitAccess -> requestSimple(productId, permission)
        }
    }

    override suspend fun revoke(productId: ProductId, permission: RemotePermission) {
        when (permission) {
            is RemotePermission.NetworkAccess -> networkAccessHandler.revoke(productId, permission)
            is RemotePermission.WebRtcAccess,
            is RemotePermission.ChainSubmitAccess,
            is RemotePermission.StatementSubmitAccess,
            is RemotePermission.PreimageSubmitAccess -> repository.revoke(productId, permission)
        }
    }

    private suspend fun requestSimple(productId: ProductId, permission: RemotePermission): Boolean {
        if (repository.isGranted(productId, permission)) return true

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
}
