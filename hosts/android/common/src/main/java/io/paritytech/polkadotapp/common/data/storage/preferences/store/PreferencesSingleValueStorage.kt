package io.paritytech.polkadotapp.common.data.storage.preferences.store

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import kotlinx.coroutines.flow.map

internal class PreferencesSingleValueStorage<T>(
    private val serializer: PreferencesSingleValueSerializer<T>,
    private val preferences: Preferences,
    private val key: String,
    private val default: T?,
) : SingleValueStorage<T> {
    override fun valueFlow() = preferences.keyFlow(key)
        .map { preferences.getString(it)?.run(serializer::fromString) ?: default }

    override suspend fun getValue(): T? {
        return preferences.getString(key)?.let(serializer::fromString)
    }

    override suspend fun requireValue(): T = requireNotNull(getValue() ?: default) {
        "Required value is null"
    }

    override suspend fun saveValue(value: T) {
        preferences.putString(key, serializer.toString(value))
    }

    override suspend fun removeValue() {
        preferences.removeField(key)
    }
}
