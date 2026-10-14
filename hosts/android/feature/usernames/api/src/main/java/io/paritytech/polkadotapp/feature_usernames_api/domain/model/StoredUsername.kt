package io.paritytech.polkadotapp.feature_usernames_api.domain.model

data class StoredUsername(
    val fullUsername: Username?,
    val liteUsername: Username,
    val isOnChain: Boolean
) {
    val username: Username
        get() = fullUsername ?: liteUsername
}
