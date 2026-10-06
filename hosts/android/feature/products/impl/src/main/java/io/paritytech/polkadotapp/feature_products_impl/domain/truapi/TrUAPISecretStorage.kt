package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.HostSecretStorage
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.truapi.SecretCoreStorageKey
import uniffi.truapi.secretCoreStorageKeyIdentifier
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class TrUAPIStorageGate @Inject constructor(private val preferences: Preferences) {
    private val mutex = Mutex()

    suspend fun <T> withStorage(operation: () -> T): T = mutex.withLock {
        currentCoroutineContext().ensureActive()
        // A failed commit can leave newer data visible in SharedPreferences memory.
        check(preferences.edit().commit()) { "Core storage persistence is unavailable" }
        operation()
    }
}

@Singleton
class TrUAPISecretStorage @Inject constructor(
    private val preferences: EncryptedPreferences,
    private val gate: TrUAPIStorageGate,
    private val deviceKeys: OurDeviceKeypairProvider,
) : HostSecretStorage {
    override suspend fun read(key: SecretCoreStorageKey): ByteArray? {
        currentCoroutineContext().ensureActive()
        if (key == SecretCoreStorageKey.DeviceEncryptionKey) {
            return deviceKeys.get().privateKey.bytes.value
        }
        return gate.withStorage { readCommittedStorageValue(preferences, secretCoreStorageKeyIdentifier(key)) }
    }

    override suspend fun write(key: SecretCoreStorageKey, value: ByteArray) {
        require(key != SecretCoreStorageKey.DeviceEncryptionKey) { "Device encryption identity is managed by the device key provider" }
        gate.withStorage { writeCommittedStorageValue(preferences, secretCoreStorageKeyIdentifier(key), value) }
    }

    override suspend fun clear(key: SecretCoreStorageKey) {
        require(key != SecretCoreStorageKey.DeviceEncryptionKey) { "Device encryption identity is managed by the device key provider" }
        gate.withStorage { preferences.removeKeyCommitted(secretCoreStorageKeyIdentifier(key)) }
    }
}

@OptIn(ExperimentalStdlibApi::class)
internal fun readCommittedStorageValue(preferences: EncryptedPreferences, key: String): ByteArray? {
    val stored = preferences.getDecryptedStringOrThrow(key) ?: return null
    require(stored.startsWith("v1:")) { "Invalid core storage value encoding" }
    return stored.removePrefix("v1:").hexToByteArray()
}

@OptIn(ExperimentalStdlibApi::class)
internal fun writeCommittedStorageValue(preferences: EncryptedPreferences, key: String, value: ByteArray) {
    preferences.putEncryptedStringCommitted(key, "v1:" + value.toHexString())
}
