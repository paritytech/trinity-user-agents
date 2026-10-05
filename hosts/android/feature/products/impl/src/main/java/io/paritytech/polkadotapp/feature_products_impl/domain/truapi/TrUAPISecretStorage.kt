package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.HostSecretStorage
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import uniffi.truapi.SecretCoreStorageKey
import uniffi.truapi.secretCoreStorageKeyIdentifier
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class TrUAPISecretStorage @Inject constructor(
    private val preferences: EncryptedPreferences,
    private val deviceKeys: OurDeviceKeypairProvider,
) : HostSecretStorage {
    @OptIn(ExperimentalStdlibApi::class)
    override suspend fun read(key: SecretCoreStorageKey): ByteArray? {
        if (key is SecretCoreStorageKey.DeviceEncryptionKey) {
            return deviceKeys.get().privateKey.bytes.value
        }
        val value = preferences.getDecryptedStringOrThrow(secretCoreStorageKeyIdentifier(key)) ?: return null
        require(value.startsWith(VALUE_TAG)) { "Corrupt protected host record" }
        return value.removePrefix(VALUE_TAG).hexToByteArray()
    }

    @OptIn(ExperimentalStdlibApi::class)
    override suspend fun write(key: SecretCoreStorageKey, value: ByteArray) {
        require(key !is SecretCoreStorageKey.DeviceEncryptionKey) { "Shared device identity is owned by its protected provider" }
        preferences.putEncryptedStringCommitted(secretCoreStorageKeyIdentifier(key), VALUE_TAG + value.toHexString())
    }

    override suspend fun clear(key: SecretCoreStorageKey) {
        require(key !is SecretCoreStorageKey.DeviceEncryptionKey) { "Shared device identity cannot be removed by host cleanup" }
        preferences.removeKeyCommitted(secretCoreStorageKeyIdentifier(key))
    }

    private companion object {
        const val VALUE_TAG = "v1:"
    }
}
