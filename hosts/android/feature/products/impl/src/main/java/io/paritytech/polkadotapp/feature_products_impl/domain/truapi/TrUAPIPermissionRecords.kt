package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus
import uniffi.truapi.PermissionRecord
import uniffi.truapi.RemotePermission
import uniffi.truapi.RemotePermissionRequest

internal fun ProductPermission.toCoreRequest(): PermissionAuthorizationRequest = when (this) {
    is ProductPermission.DeviceCapability -> PermissionAuthorizationRequest.Device(HostDevicePermissionRequest.entries.first { it.toCapability() == capability })
    is ProductPermission.AccountAccess -> PermissionAuthorizationRequest.AccountAccess(targetProductId)
    ProductPermission.UserIdentityAccess -> PermissionAuthorizationRequest.IdentityDisclosure
    ProductPermission.BalanceAccess -> error("Balance disclosure is not a persisted TrUAPI permission")
    is ProductPermission.RemotePermission -> PermissionAuthorizationRequest.Remote(RemotePermissionRequest(when (this) {
        is ProductPermission.RemotePermission.NetworkAccess -> RemotePermission.Remote(listOf(domain))
        ProductPermission.RemotePermission.WebRtcAccess -> RemotePermission.WebRtc
        ProductPermission.RemotePermission.ChainSubmitAccess -> RemotePermission.ChainSubmit
        ProductPermission.RemotePermission.StatementSubmitAccess -> RemotePermission.StatementSubmit
        ProductPermission.RemotePermission.PreimageSubmitAccess -> RemotePermission.PreimageSubmit
    }))
}

internal fun PermissionRecord.toProductPermissions(): List<ProductPermissionStatus> {
    val permissions = when (val permission = request) {
        is PermissionAuthorizationRequest.Device -> listOf(ProductPermission.DeviceCapability(permission.v1.toCapability()))
        is PermissionAuthorizationRequest.AccountAccess -> listOf(ProductPermission.AccountAccess(permission.targetProductId))
        PermissionAuthorizationRequest.IdentityDisclosure -> listOf(ProductPermission.UserIdentityAccess)
        is PermissionAuthorizationRequest.Remote -> when (val remote = permission.v1.permission) {
            is RemotePermission.Remote -> remote.domains.map { ProductPermission.RemotePermission.NetworkAccess(it) }
            RemotePermission.WebRtc -> listOf(ProductPermission.RemotePermission.WebRtcAccess)
            RemotePermission.ChainSubmit -> listOf(ProductPermission.RemotePermission.ChainSubmitAccess)
            RemotePermission.StatementSubmit -> listOf(ProductPermission.RemotePermission.StatementSubmitAccess)
            RemotePermission.PreimageSubmit -> listOf(ProductPermission.RemotePermission.PreimageSubmitAccess)
        }
    }
    return permissions.map { ProductPermissionStatus(it, status == PermissionAuthorizationStatus.AUTHORIZED) }
}
