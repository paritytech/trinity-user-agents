package io.paritytech.polkadotapp.common.data.storage.preferences.encrypted

import kotlinx.coroutines.flow.Flow

interface EncryptedPreferences {
    fun putEncryptedString(
        field: String,
        value: String,
    )

    fun getDecryptedString(field: String): String?

    fun hasKey(field: String): Boolean

    fun removeKey(field: String)

    /** Every stored field whose name starts with [prefix], readable or not. */
    fun fieldsStartingWith(prefix: String): Set<String>

    fun decryptedStringFlow(field: String): Flow<String?>
}
