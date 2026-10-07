package io.paritytech.polkadotapp.common.data.storage.preferences.encrypted

import kotlinx.coroutines.flow.Flow

interface EncryptedPreferences {
    /** Physical namespace shared by adapters of these same preference slots. */
    val storageIdentifier: String

    /** Return only after durable persistence, throwing if encryption or commit fails. */
    fun putEncryptedStringCommitted(field: String, value: String)
    fun removeKeyCommitted(field: String)

    fun putEncryptedString(
        field: String,
        value: String,
    )

    fun getDecryptedString(field: String): String?

    fun hasKey(field: String): Boolean

    fun removeKey(field: String)

    fun decryptedStringFlow(field: String): Flow<String?>
}
