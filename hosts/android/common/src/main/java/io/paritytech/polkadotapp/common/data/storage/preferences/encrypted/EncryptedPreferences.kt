package io.paritytech.polkadotapp.common.data.storage.preferences.encrypted

import kotlinx.coroutines.flow.Flow

interface EncryptedPreferences {
    val storageIdentifier: String
        get() = "${this::class.qualifiedName}:${System.identityHashCode(this)}"

    fun keys(): Set<String>

    fun putEncryptedString(
        field: String,
        value: String,
    )

    fun putEncryptedStringCommitted(
        field: String,
        value: String,
    ) = putEncryptedString(field, value)

    fun getDecryptedString(field: String): String?

    fun hasKey(field: String): Boolean

    fun removeKey(field: String)

    fun removeKeyCommitted(field: String) = removeKey(field)

    fun decryptedStringFlow(field: String): Flow<String?>
}
