package io.paritytech.polkadotapp.tools_push_notifications_impl.data

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueSerializer

typealias LocalPushTokenStorage = SingleValueStorage<String>

private const val KEY = "PushToken"

fun SingleValueStorageFactory.pushTokenStorage(): LocalPushTokenStorage {
    return preferences(
        key = KEY,
        serializer = PreferencesSingleValueSerializer.from(
            toString = { it },
            fromString = { it }
        ),
        default = null
    )
}
