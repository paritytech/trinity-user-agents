package io.paritytech.polkadotapp.feature_usernames_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueSerializer
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalFullUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

private const val KEY = "Username"
private const val FULL_KEY = "FullUsername"

fun SingleValueStorageFactory.usernameStorage(): LocalUsernameStorage {
    return preferences(
        key = KEY,
        serializer = PreferencesSingleValueSerializer.from(
            toString = Username::getDisplayUsername,
            fromString = Username::fromFullValue
        ),
        default = null
    )
}

fun SingleValueStorageFactory.fullUsernameStorage(): LocalFullUsernameStorage {
    val storage = preferences(
        key = FULL_KEY,
        serializer = PreferencesSingleValueSerializer.from(
            toString = Username::getDisplayUsername,
            fromString = Username::fromFullValue
        ),
        default = null
    )
    return LocalFullUsernameStorage(storage)
}
