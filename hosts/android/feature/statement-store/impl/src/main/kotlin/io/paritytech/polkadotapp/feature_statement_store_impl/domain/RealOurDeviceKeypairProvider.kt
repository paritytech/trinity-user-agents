package io.paritytech.polkadotapp.feature_statement_store_impl.domain

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.domain.model.X25519KeyPair
import io.paritytech.polkadotapp.common.domain.model.X25519PrivateKey
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.utils.X25519KeyGenerator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

private const val OUR_DEVICE_PRIVATE_KEY = "our_device_private_key"

@Singleton
class RealOurDeviceKeypairProvider @Inject constructor(
    private val encryptedPreferences: EncryptedPreferences,
    private val keyGenerator: X25519KeyGenerator,
) : OurDeviceKeypairProvider {
    private val mutex = Mutex()

    @Volatile
    private var cached: X25519KeyPair? = null

    override suspend fun get(): X25519KeyPair {
        cached?.let { return it }

        return mutex.withLock {
            cached ?: load().also { cached = it }
        }
    }

    override suspend fun publicKey(): X25519PublicKey {
        return get().publicKey
    }

    private fun load(): X25519KeyPair {
        val storedPrivateKey = encryptedPreferences.getDecryptedString(OUR_DEVICE_PRIVATE_KEY)
        if (storedPrivateKey != null) {
            return keyGenerator.createKeyPair(X25519PrivateKey.fromDerivedBytes(storedPrivateKey.fromHex()))
        }

        val fresh = keyGenerator.generateRandomKeypair()
        encryptedPreferences.putEncryptedString(OUR_DEVICE_PRIVATE_KEY, fresh.privateKey.bytes.value.toHexString())
        return fresh
    }
}
