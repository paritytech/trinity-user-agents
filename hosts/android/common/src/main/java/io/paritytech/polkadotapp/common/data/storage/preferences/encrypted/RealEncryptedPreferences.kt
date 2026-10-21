package io.paritytech.polkadotapp.common.data.storage.preferences.encrypted

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.utils.flowOfAll
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class RealEncryptedPreferences @Inject constructor(
    private val preferences: Preferences,
    private val encryptionUtil: EncryptionUtil
) : EncryptedPreferences {
    override fun putEncryptedString(
        field: String,
        value: String,
    ) {
        preferences.putString(field, encryptionUtil.encrypt(value))
    }

    override fun putEncryptedStringCommitted(field: String, value: String) {
        val ciphertext = encryptionUtil.encryptOrThrow(value)
        val editor = preferences.edit()
        editor.putString(field, ciphertext)
        check(editor.commit()) { "Encrypted preference write was not committed" }
    }

    override fun getDecryptedStringOrThrow(field: String): String? =
        preferences.getString(field)?.let(encryptionUtil::decryptOrThrow)

    override fun removeKeyCommitted(field: String) {
        val editor = preferences.edit()
        editor.remove(field)
        check(editor.commit()) { "Encrypted preference removal was not committed" }
    }

    override fun getDecryptedString(field: String): String? {
        val encryptedString = preferences.getString(field)
        return encryptedString?.let { encryptionUtil.decrypt(it) }
    }

    override fun hasKey(field: String): Boolean {
        return preferences.contains(field)
    }

    override fun removeKey(field: String) {
        preferences.removeField(field)
    }

    override fun decryptedStringFlow(field: String) = flowOfAll {
        preferences.stringFlow(field)
            .map { encryptedString ->
                encryptedString?.let { encryptionUtil.decrypt(it) }
            }
    }
}
