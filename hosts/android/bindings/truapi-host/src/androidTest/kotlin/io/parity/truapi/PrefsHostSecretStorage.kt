package io.parity.truapi

import android.content.SharedPreferences
import uniffi.truapi.SecretCoreStorageKey
import uniffi.truapi.secretCoreStorageKeyIdentifier

/** Diagnostics fixture; production hosts keep these values in protected storage. */
class PrefsHostSecretStorage(private val prefs: SharedPreferences) : HostSecretStorage {
    @OptIn(ExperimentalStdlibApi::class)
    override suspend fun read(key: SecretCoreStorageKey): ByteArray? =
        prefs.getString(secretCoreStorageKeyIdentifier(key), null)?.hexToByteArray()

    override suspend fun write(key: SecretCoreStorageKey, value: ByteArray) {
        check(prefs.edit().putString(secretCoreStorageKeyIdentifier(key), bytesToHex(value)).commit())
    }

    override suspend fun clear(key: SecretCoreStorageKey) {
        check(prefs.edit().remove(secretCoreStorageKeyIdentifier(key)).commit())
    }
}


@OptIn(ExperimentalStdlibApi::class)
private fun bytesToHex(bytes: ByteArray): String = bytes.toHexString()
