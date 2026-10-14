package io.paritytech.polkadotapp.common.data.storage.preferences.store

interface PreferencesSingleValueSerializer<T> {
    companion object {
        fun <T> from(
            toString: (T) -> String,
            fromString: (String) -> T
        ): PreferencesSingleValueSerializer<T> {
            return DelegatingPreferencesSingleValueSerializer(toString, fromString)
        }
    }

    fun toString(value: T): String

    fun fromString(raw: String): T
}

private class DelegatingPreferencesSingleValueSerializer<T>(
    private val toStringDelegate: (T) -> String,
    private val fromStringDelegate: (String) -> T
) : PreferencesSingleValueSerializer<T> {
    override fun toString(value: T): String {
        return toStringDelegate(value)
    }

    override fun fromString(raw: String): T {
        return fromStringDelegate(raw)
    }
}
