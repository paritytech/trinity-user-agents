package io.paritytech.polkadotapp.common.data.storage.preferences.encrypted

import kotlinx.coroutines.flow.Flow

interface EncryptedPreferences {
    fun putEncryptedString(
        field: String,
        value: String,
    )

    fun putEncryptedStringCommitted(field: String, value: String)

    fun getDecryptedString(field: String): String?

    fun getDecryptedStringOrThrow(field: String): String?

    fun removeKeyCommitted(field: String)

    fun hasKey(field: String): Boolean

    fun removeKey(field: String)

    fun decryptedStringFlow(field: String): Flow<String?>
}
