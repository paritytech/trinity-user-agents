package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models

sealed interface RemotePermissionRequest {
    data class Remote(val domains: List<String>) : RemotePermissionRequest
    data object WebRtc : RemotePermissionRequest
    data object ChainSubmit : RemotePermissionRequest
    data object StatementSubmit : RemotePermissionRequest
    data object PreimageSubmit : RemotePermissionRequest
}
