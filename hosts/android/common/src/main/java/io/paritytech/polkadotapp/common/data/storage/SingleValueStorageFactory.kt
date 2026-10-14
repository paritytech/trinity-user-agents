package io.paritytech.polkadotapp.common.data.storage

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueSerializer
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueStorage
import javax.inject.Inject

class SingleValueStorageFactory @Inject constructor(
    private val preferences: Preferences,
) {
    fun <T> preferences(
        key: String,
        default: T?,
        serializer: PreferencesSingleValueSerializer<T>
    ): SingleValueStorage<T> {
        return PreferencesSingleValueStorage(serializer, preferences, key, default)
    }
}
