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
