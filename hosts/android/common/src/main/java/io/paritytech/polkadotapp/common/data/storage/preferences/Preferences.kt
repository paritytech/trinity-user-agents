package io.paritytech.polkadotapp.common.data.storage.preferences

import kotlinx.coroutines.flow.Flow

typealias InitialValueProducer<T> = suspend () -> T

interface Preferences {
    fun contains(field: String): Boolean

    fun putString(
        field: String,
        value: String?,
    )

    fun getString(
        field: String,
        defaultValue: String,
    ): String

    fun getString(field: String): String?

    fun putBoolean(
        field: String,
        value: Boolean,
    )

    fun getBoolean(
        field: String,
        defaultValue: Boolean,
    ): Boolean

    fun putInt(
        field: String,
        value: Int,
    )

    fun getInt(
        field: String,
        defaultValue: Int,
    ): Int

    fun putLong(
        field: String,
        value: Long,
    )

    fun getLong(
        field: String,
        defaultValue: Long,
    ): Long

    fun putStringSet(
        field: String,
        value: Set<String>
    )

    fun getStringSet(
        field: String
    ): Set<String>

    fun removeField(field: String)

    fun stringFlow(
        field: String,
        initialValueProducer: InitialValueProducer<String>? = null,
    ): Flow<String?>

    fun booleanFlow(
        field: String,
        defaultValue: Boolean,
    ): Flow<Boolean>

    fun longFlow(
        field: String,
        defaultValue: Long
    ): Flow<Long>

    fun intFlow(
        field: String,
        defaultValue: Int
    ): Flow<Int>

    fun stringSetFlow(
        field: String
    ): Flow<Set<String>>

    fun keyFlow(key: String): Flow<String>

    fun keysFlow(vararg keys: String): Flow<List<String>>

    fun edit(): Editor
}

fun Preferences.edit(edit: Editor.() -> Unit) {
    edit().apply(edit).apply()
}

interface Editor {
    fun putString(
        field: String,
        value: String?,
    )

    fun putBoolean(
        field: String,
        value: Boolean,
    )

    fun putInt(
        field: String,
        value: Int,
    )

    fun putLong(
        field: String,
        value: Long,
    )

    fun remove(field: String)

    fun apply()
}
