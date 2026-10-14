package io.paritytech.polkadotapp.common.data.keypair

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.SubstrateKeypairFactory
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import java.security.SecureRandom
import javax.inject.Inject
import javax.inject.Singleton

interface ClientKeypairStore {
    fun getOrGenerate(): Sr25519Keypair
}

@Singleton
class RealClientKeypairStore @Inject constructor(
    private val encryptedPreferences: EncryptedPreferences,
) : ClientKeypairStore {
    @Volatile
    private var cached: Sr25519Keypair? = null

    @Synchronized
    override fun getOrGenerate(): Sr25519Keypair {
        cached?.let { return it }

        val stored = readPersisted()
        if (stored != null) {
            cached = stored
            return stored
        }

        val fresh = generateFresh()
        persist(fresh)
        cached = fresh
        return fresh
    }

    private fun readPersisted(): Sr25519Keypair? {
        val blob = encryptedPreferences.getDecryptedString(KEY_KEYPAIR_BLOB)?.takeUnless { it.isEmpty() }
            ?: return null

        return runCatching {
            val parts = blob.split(BLOB_DELIMITER)
            require(parts.size == EXPECTED_PARTS) { "Invalid keypair blob: ${parts.size} parts" }
            Sr25519Keypair(
                publicKey = parts[0].fromHex(),
                privateKey = parts[1].fromHex(),
                nonce = parts[2].fromHex(),
            )
        }.getOrNull()
    }

    private fun persist(keypair: Sr25519Keypair) {
        val blob = listOf(
            keypair.publicKey.toHexString(withPrefix = false),
            keypair.privateKey.toHexString(withPrefix = false),
            keypair.nonce.toHexString(withPrefix = false),
        ).joinToString(BLOB_DELIMITER)
        encryptedPreferences.putEncryptedString(KEY_KEYPAIR_BLOB, blob)
    }

    private fun generateFresh(): Sr25519Keypair {
        val seed = ByteArray(SEED_LENGTH).also(SecureRandom()::nextBytes)
        return SubstrateKeypairFactory.generate(EncryptionType.SR25519, seed, junctions = emptyList()) as Sr25519Keypair
    }

    companion object {
        private const val KEY_KEYPAIR_BLOB = "jwt_client_keypair_v1"
        private const val BLOB_DELIMITER = ":"
        private const val EXPECTED_PARTS = 3
        private const val SEED_LENGTH = 32
    }
}
